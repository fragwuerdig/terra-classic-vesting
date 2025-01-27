use std::cmp::min;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, CosmosMsg, StdResult, Storage, Timestamp, Uint128, Uint64};
use crate::denom::CheckedDenom;
use cw_storage_plus::Item;
use wynd_utils::{Curve, PiecewiseLinear, SaturatingLinear};

use crate::error::ContractError;

pub struct Payment<'a> {
    vesting: Item<'a, Vest>,
}

#[cw_serde]
pub struct Vest {
    /// vested(t), where t is seconds since start_time.
    vested: Curve,
    start_time: Timestamp,

    pub status: Status,
    pub recipient: Addr,
    pub denom: CheckedDenom,

    /// The number of tokens that have been claimed by the vest receiver.
    pub claimed: Uint128,

    pub title: String,
    pub description: Option<String>,
}

#[cw_serde]
pub enum Status {
    Unfunded,
    Funded,
    Canceled,
}

#[cw_serde]
pub enum Schedule {
    /// Vests linearally from `0` to `total`.
    SaturatingLinear,
    /// Vests by linearally interpolating between the provided
    /// (seconds, amount) points. The first amount must be zero and
    /// the last amount the total vesting amount. `seconds` are
    /// seconds since the vest start time.
    ///
    /// There is a problem in the underlying Curve library that
    /// doesn't allow zero start values, so the first value of
    /// `seconds` must be > 1. To start at a particular time (if you
    /// need that level of percision), subtract one from the true
    /// start time, and make the first `seconds` value `1`.
    ///
    /// <https://github.com/cosmorama/wynddao/pull/4>
    PiecewiseLinear(Vec<(u64, Uint128)>),
}

pub struct VestInit {
    pub total: Uint128,
    pub schedule: Schedule,
    pub start_time: Timestamp,
    pub duration_seconds: u64,
    pub denom: CheckedDenom,
    pub recipient: Addr,
    pub title: String,
    pub description: Option<String>,
}

impl<'a> Payment<'a> {
    pub const fn new(
        vesting_prefix: &'a str
    ) -> Self {
        Self {
            vesting: Item::new(vesting_prefix),
        }
    }

    /// Validates its arguments and initializes the payment. Returns
    /// the underlying vest.
    pub fn initialize(
        &self,
        storage: &mut dyn Storage,
        init: VestInit,
    ) -> Result<Vest, ContractError> {
        let v = Vest::new(init)?;
        self.vesting.save(storage, &v)?;
        Ok(v)
    }

    pub fn get_vest(&self, storage: &dyn Storage) -> StdResult<Vest> {
        self.vesting.load(storage)
    }

    /// calculates the number of liquid tokens avaliable.
    fn liquid(&self, vesting: &Vest) -> Uint128 {
        match vesting.status {
            Status::Unfunded => Uint128::zero(),
            Status::Funded => vesting.total() - vesting.claimed,
            Status::Canceled => Uint128::zero(),
        }
    }

    /// Gets the current number tokens that may be distributed to the
    /// vestee.
    pub fn distributable(
        &self,
        _storage: &dyn Storage,
        vesting: &Vest,
        t: Timestamp,
    ) -> StdResult<Uint128> {
        let liquid = self.liquid(vesting);
        let claimable = vesting.vested(t) - vesting.claimed;
        Ok(min(liquid, claimable))
    }

    /// Distributes vested tokens. If a specific amount is
    /// requested, that amount will be distributed, otherwise all
    /// tokens currently avaliable for distribution will be
    /// transfered.
    pub fn distribute(
        &self,
        storage: &mut dyn Storage,
        t: Timestamp,
        request: Option<Uint128>,
    ) -> Result<CosmosMsg, ContractError> {
        let vesting = self.vesting.load(storage)?;

        let distributable = self.distributable(storage, &vesting, t)?;
        let request = request.unwrap_or(distributable);

        let mut vesting = vesting;
        vesting.claimed += request;
        self.vesting.save(storage, &vesting)?;

        if request > distributable || request.is_zero() {
            Err(ContractError::InvalidWithdrawal {
                request,
                claimable: distributable,
            })
        } else {
            Ok(vesting
                .denom
                .get_transfer_to_message(&vesting.recipient, request)?)
        }
    }

    /// Cancels the vesting payment. The current amount vested becomes
    /// the total amount that will ever vest. note that canceling does
    /// not impact already vested tokens.
    pub fn cancel(
        &self,
        storage: &mut dyn Storage,
        t: Timestamp,
        total_balance: Uint128,
    ) -> Result<Vec<CosmosMsg>, ContractError> {
        let mut vesting = self.vesting.load(storage)?;
        if matches!(vesting.status, Status::Canceled { .. }) {
            Err(ContractError::Cancelled {})
        } else {

            let mut msgs = vec![];

            // the outstanding amount that the vestee is entitled to
            let to_vestee = vesting.vested(t) - vesting.claimed;
            if to_vestee > Uint128::zero() {
                msgs.push(
                    vesting
                        .denom
                        .get_transfer_to_message(&vesting.recipient, to_vestee)?,
                )
            }

            // the amount that the Community Pool is entitled to
            let to_owner = total_balance - to_vestee;
            if to_owner > Uint128::zero() {
                msgs.push(vesting.denom.get_fund_cp_message(to_owner)?);
            }

            vesting.cancel(t);
            self.vesting.save(storage, &vesting)?;

            Ok(msgs)
        }
    }

    pub fn set_funded(&self, storage: &mut dyn Storage) -> Result<(), ContractError> {
        let mut v = self.vesting.load(storage)?;
        debug_assert!(v.status == Status::Unfunded);
        v.status = Status::Funded;
        self.vesting.save(storage, &v)?;
        Ok(())
    }

    /// Returns the duration of the vesting agreement (not the
    /// remaining time) in seconds, or `None` if the vest has been cancelled.
    pub fn duration(&self, storage: &dyn Storage) -> StdResult<Option<Uint64>> {
        self.vesting.load(storage).map(|v| v.duration())
    }
}

impl Vest {
    pub fn new(init: VestInit) -> Result<Self, ContractError> {
        if init.total.is_zero() {
            Err(ContractError::ZeroVest)
        } else if init.duration_seconds == 0 {
            Err(ContractError::Instavest)
        } else {
            Ok(Self {
                claimed: Uint128::zero(),
                vested: init
                    .schedule
                    .into_curve(init.total, init.duration_seconds)?,
                start_time: init.start_time,
                denom: init.denom,
                recipient: init.recipient,
                status: Status::Unfunded,
                title: init.title,
                description: init.description,
            })
        }
    }

    /// Gets the total number of tokens that will vest as part of this
    /// payment.
    pub fn total(&self) -> Uint128 {
        Uint128::new(self.vested.range().1)
    }

    /// Gets the number of tokens that have vested at `time`.
    pub fn vested(&self, t: Timestamp) -> Uint128 {
        let elapsed = t.seconds().saturating_sub(self.start_time.seconds());
        self.vested.value(elapsed)
    }

    /// Cancels the current vest. No additional tokens will vest after `t`.
    pub fn cancel(&mut self, t: Timestamp) {
        debug_assert!(!matches!(self.status, Status::Canceled { .. }));

        self.status = Status::Canceled;
        self.vested = Curve::Constant { y: self.vested(t) };
    }

    /// Gets the duration of the vest. For constant curves, `None` is
    /// returned.
    pub fn duration(&self) -> Option<Uint64> {
        let (start, end) = match &self.vested {
            Curve::Constant { .. } => return None,
            Curve::SaturatingLinear(SaturatingLinear { min_x, max_x, .. }) => (*min_x, *max_x),
            Curve::PiecewiseLinear(PiecewiseLinear { steps }) => {
                (steps[0].0, steps[steps.len() - 1].0)
            }
        };
        Some(Uint64::new(end - start))
    }
}

impl Schedule {
    /// The vesting schedule tracks vested(t), so for a curve to be
    /// valid:
    ///
    /// 1. it must start at 0,
    /// 2. it must end at total,
    /// 3. it must never decrease.
    ///
    /// Piecewise curves must have at least two steps. One step would
    /// be a constant vest (why would you want this?).
    ///
    /// A schedule is valid if `total` is zero: nothing will ever be
    /// paid out. Consumers should consider validating that `total` is
    /// non-zero.
    pub fn into_curve(self, total: Uint128, duration_seconds: u64) -> Result<Curve, ContractError> {
        let c = match self {
            Schedule::SaturatingLinear => {
                Curve::saturating_linear((0, 0), (duration_seconds, total.u128()))
            }
            Schedule::PiecewiseLinear(steps) => {
                if steps.len() < 2 {
                    return Err(ContractError::ConstantVest);
                }
                Curve::PiecewiseLinear(wynd_utils::PiecewiseLinear { steps })
            }
        };
        c.validate_monotonic_increasing()?; // => max >= curve(t) \forall t
        let range = c.range();
        if range != (0, total.u128()) {
            return Err(ContractError::VestRange {
                min: Uint128::new(range.0),
                max: Uint128::new(range.1),
            });
        }
        Ok(c)
    }
}

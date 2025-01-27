use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Timestamp, Uint128};
use crate::denom::UncheckedDenom;
use cw_ownable::cw_ownable_execute;

use crate::payment::Schedule;

#[cw_serde]
pub struct InstantiateMsg {
    /// The mandatory owner address of the contract. The owner
    /// should be the governance module of the Terra Classic
    /// blockchain.
    pub owner: String,
    /// The receiver address of the vesting tokens.
    pub recipient: String,
    /// The a name or title for this payment.
    pub title: String,
    /// A description for the payment to provide more context.
    pub description: Option<String>,
    /// The total amount of tokens to be vested.
    pub total: Uint128,
    /// The type and denom of token being vested.
    pub denom: UncheckedDenom,
    /// The vesting schedule, can be either `SaturatingLinear` vesting
    /// (which vests evenly over time), or `PiecewiseLinear` which can
    /// represent a more complicated vesting schedule.
    pub schedule: Schedule,
    /// The time to start vesting, or None to start vesting when the
    /// contract is instantiated. `start_time` may be in the past,
    /// though the contract checks that `start_time +
    /// vesting_duration_seconds > now`. Otherwise, this would amount
    /// to a regular fund transfer.
    pub start_time: Option<Timestamp>,
    /// The length of the vesting schedule in seconds. Must be
    /// non-zero, though one second vesting durations are
    /// allowed. This may be combined with a `start_time` in the
    /// future to create an agreement that instantly vests at a time
    /// in the future, and allows the receiver to stake vesting tokens
    /// before the agreement completes.
    ///
    /// See `suite_tests/tests.rs`
    /// `test_almost_instavest_in_the_future` for an example of this.
    pub vesting_duration_seconds: u64,
}

#[cw_ownable_execute]
#[cw_serde]
pub enum ExecuteMsg {
    /// After the contract has received the exact amount of tokens
    /// to be vested, anyone can call this method to mark the contract
    /// as funded so that the vesting schedule can become active.
    Fund {},
    /// Distribute vested tokens to the vest receiver. Anyone may call
    /// this method.
    Distribute {
        /// The amount of tokens to distribute. If none are specified
        /// all claimable tokens will be distributed.
        amount: Option<Uint128>,
    },
    /// Cancels the vesting payment. The current amount vested becomes
    /// the total amount that will ever vest. Note that canceling does
    /// not impact already vested tokens.
    ///
    /// The amounts that the vestee and the Community Pool are entitled
    /// to are calculated and transferred to the respective parties.
    Cancel {},
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Get the current ownership.
    #[returns(::cw_ownable::Ownership<::cosmwasm_std::Addr>)]
    Ownership {},
    /// Returns information about the vesting contract and the
    /// status of the payment.
    #[returns(crate::payment::Vest)]
    Info {},
    /// Returns the number of tokens currently claimable by the
    /// vestee. This is the minimum of the number of unstaked tokens
    /// in the contract, and the number of tokens that have been
    /// vested at time t.
    #[returns(::cosmwasm_std::Uint128)]
    Distributable {
        /// The time or none to use the current time.
        t: Option<Timestamp>,
    },
    /// Gets the current value of `vested(t)`. If `t` is `None`, the
    /// current time is used.
    #[returns(::cosmwasm_std::Uint128)]
    Vested { t: Option<Timestamp> },
    /// Gets the total amount that will ever vest, `max(vested(t))`.
    ///
    /// Note that if the contract is canceled at time c, this value
    /// will change to `vested(c)`. Thus, it can not be assumed to be
    /// constant over the contract's lifetime.
    #[returns(::cosmwasm_std::Uint128)]
    TotalToVest {},
    /// Gets the amount of time between the vest starting, and it
    /// completing. Returns `None` if the vest has been cancelled.
    #[returns(Option<::cosmwasm_std::Uint64>)]
    VestDuration {},
}

#[cw_serde]
pub struct MigrateWithdrawBalance {
    pub amount: Uint128,
    pub recipient: String,
    pub force: Option<bool>,
}

#[cw_serde]
pub struct MigrateMsg {
    pub withdraw: Option<MigrateWithdrawBalance>,
}
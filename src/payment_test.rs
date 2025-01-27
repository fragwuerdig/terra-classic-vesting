#[cfg(test)]
use cosmwasm_std::{testing::mock_dependencies, Addr, BankMsg, Coin, CosmosMsg, DistributionMsg, Timestamp, Uint128};

#[cfg(test)]
use crate::denom::CheckedDenom;

#[cfg(test)]
use wynd_utils::CurveError;

#[cfg(test)]
use crate::{
    error::ContractError,
    payment::{Payment, Schedule, Vest, VestInit},
};

#[cfg(test)]
impl Default for VestInit {
    fn default() -> Self {
        VestInit {
            total: Uint128::new(100_000_000),
            schedule: Schedule::SaturatingLinear,
            start_time: Timestamp::from_seconds(0),
            duration_seconds: 100,
            denom: CheckedDenom::Native("native".to_string()),
            recipient: Addr::unchecked("recv"),
            title: "title".to_string(),
            description: Some("desc".to_string()),
        }
    }
}

#[test]
fn test_distribute_funded() {
    let storage = &mut mock_dependencies().storage;
    let payment = Payment::new("vesting");

    payment.initialize(storage, VestInit::default()).unwrap();
    payment.set_funded(storage).unwrap();

    payment
        .distribute(storage, Timestamp::default().plus_seconds(10), None)
        .unwrap();
}

#[test]
fn test_distribute_nothing_to_claim() {
    let storage = &mut mock_dependencies().storage;
    let payment = Payment::new("vesting");

    payment.initialize(storage, VestInit::default()).unwrap();

    payment.set_funded(storage).unwrap();

    // Can't distribute when there is nothing to claim.
    let err = payment
        .distribute(storage, Timestamp::default(), None)
        .unwrap_err();
    assert_eq!(
        err,
        ContractError::InvalidWithdrawal {
            request: Uint128::zero(),
            claimable: Uint128::zero()
        }
    );
}

#[test]
fn test_distribute_half_way() {
    let storage = &mut mock_dependencies().storage;
    let payment = Payment::new("vesting");

    payment.initialize(storage, VestInit::default()).unwrap();

    payment.set_funded(storage).unwrap();
    // 50% of the way through, max claimable is 1/2 total.
    let err = payment
        .distribute(
            storage,
            Timestamp::from_seconds(50),
            Some(Uint128::new(50_000_001)),
        )
        .unwrap_err();
    assert_eq!(
        err,
        ContractError::InvalidWithdrawal {
            request: Uint128::new(50_000_001),
            claimable: Uint128::new(50_000_000)
        }
    );
}

#[test]
fn test_distribute() {
    let storage = &mut mock_dependencies().storage;
    let payment = Payment::new("vesting");

    payment.initialize(storage, VestInit::default()).unwrap();

    payment.set_funded(storage).unwrap();

    // partially claiming increases claimed
    let msg = payment
        .distribute(storage, Timestamp::from_seconds(50), Some(Uint128::new(3)))
        .unwrap();

    assert_eq!(
        msg,
        payment
            .get_vest(storage)
            .unwrap()
            .denom
            .get_transfer_to_message(&Addr::unchecked("recv"), Uint128::new(3))
            .unwrap()
    );
    assert_eq!(payment.get_vest(storage).unwrap().claimed, Uint128::new(3));

    payment
        .distribute(
            storage,
            Timestamp::from_seconds(50),
            Some(Uint128::new(50_000_000 - 3)),
        )
        .unwrap();
}

#[test]
fn test_vesting_validation() {
    // Can not create vesting payment which vests zero tokens.
    let init = VestInit {
        total: Uint128::zero(),
        ..Default::default()
    };
    assert_eq!(Vest::new(init), Err(ContractError::ZeroVest {}));

    let init = VestInit {
        schedule: Schedule::PiecewiseLinear(vec![
            (0, Uint128::zero()),
            (1, Uint128::one()),
            (2, Uint128::zero()), // non-monotonic-increasing
            (3, Uint128::new(3)),
        ]),
        ..Default::default()
    };

    assert_eq!(
        Vest::new(init),
        Err(ContractError::Curve(CurveError::PointsOutOfOrder))
    );

    // Doesn't reach total.
    let init = VestInit {
        schedule: Schedule::PiecewiseLinear(vec![
            (1, Uint128::zero()),
            (2, Uint128::one()),
            (5, Uint128::new(2)),
        ]),
        ..Default::default()
    };

    assert_eq!(
        Vest::new(init),
        Err(ContractError::VestRange {
            min: Uint128::zero(),
            max: Uint128::new(2)
        })
    );
}

#[test]
fn test_cancellation() {

    let storage = &mut mock_dependencies().storage;
    let mut time = Timestamp::default();

    let init = VestInit {
        total: Uint128::new(100),
        schedule: Schedule::SaturatingLinear,
        start_time: time,
        duration_seconds: 100,
        denom: CheckedDenom::Native("uluna".to_string()),
        recipient: Addr::unchecked("recv"),
        title: "t".to_string(),
        description: Some("d".to_string()),
    };
    let payment = Payment::new("vesting");

    payment.initialize(storage, init).unwrap();
    payment.set_funded(storage).unwrap();

    time = time.plus_seconds(50);

    assert_eq!(payment.get_vest(storage).unwrap().claimed, Uint128::zero());
    assert_eq!(payment.get_vest(storage).unwrap().vested(time), Uint128::new(50));

    // cancel the payment - contract balance 1000 tokens (overfunded)
    // -> 50 are unclaimed by the vestee
    // -> 950 are returned to the community pool
    let resp = payment.cancel(storage, time, 1000u128.into()).unwrap();
    assert_eq!(resp.len(), 2);
    if let CosmosMsg::Bank(BankMsg::Send { to_address, amount }) = &resp[0] {
        assert_eq!(to_address, "recv");
        assert_eq!(amount, &[Coin::new(50u128.into(), "uluna")]);
    } else {
        panic!("unexpected message");
    }

    if let CosmosMsg::Distribution(DistributionMsg::FundCommunityPool { amount }) = &resp[1] {
        assert_eq!(amount, &[Coin::new(950u128.into(), "uluna")]);
    } else {
        panic!("unexpected message");
    }
    
}

#[test]
fn test_cancellation_no_zero_payments() {

    let storage = &mut mock_dependencies().storage;
    let mut time = Timestamp::default();

    let init = VestInit {
        total: Uint128::new(100),
        schedule: Schedule::SaturatingLinear,
        start_time: time,
        duration_seconds: 100,
        denom: CheckedDenom::Native("uluna".to_string()),
        recipient: Addr::unchecked("recv"),
        title: "t".to_string(),
        description: Some("d".to_string()),
    };
    let payment = Payment::new("vesting");

    payment.initialize(storage, init).unwrap();
    payment.set_funded(storage).unwrap();

    // vesting schedule is over
    time = time.plus_seconds(150);

    assert_eq!(payment.get_vest(storage).unwrap().claimed, Uint128::zero());
    assert_eq!(payment.get_vest(storage).unwrap().vested(time), Uint128::new(100));

    payment.distribute(storage, time, None).unwrap();

    assert_eq!(payment.get_vest(storage).unwrap().claimed, Uint128::new(100));

    // cancel the payment after schedule - contract balance 0 tokens left (not overfunded)
    // -> 100 are claimed by the vestee -> 0 to be sent to the vestee
    // -> 0 are returned to the community pool
    let resp = payment.cancel(storage, time, 0u128.into()).unwrap();
    assert_eq!(resp.len(), 0);

}

#[test]
fn test_cancellation_contract_overfunding() {

    let storage = &mut mock_dependencies().storage;
    let mut time = Timestamp::default();

    let init = VestInit {
        total: Uint128::new(100),
        schedule: Schedule::SaturatingLinear,
        start_time: time,
        duration_seconds: 100,
        denom: CheckedDenom::Native("uluna".to_string()),
        recipient: Addr::unchecked("recv"),
        title: "t".to_string(),
        description: Some("d".to_string()),
    };
    let payment = Payment::new("vesting");

    payment.initialize(storage, init).unwrap();
    payment.set_funded(storage).unwrap();

    // vesting schedule is over
    time = time.plus_seconds(150);

    assert_eq!(payment.get_vest(storage).unwrap().claimed, Uint128::zero());
    assert_eq!(payment.get_vest(storage).unwrap().vested(time), Uint128::new(100));

    payment.distribute(storage, time, None).unwrap();

    assert_eq!(payment.get_vest(storage).unwrap().claimed, Uint128::new(100));

    // cancel the payment after schedule - contract balance 10 tokens left (overfunded)
    // -> 100 are claimed by the vestee -> 0 to be sent to the vestee
    // -> 0 are returned to the community pool
    let resp = payment.cancel(storage, time, 10u128.into()).unwrap();
    assert_eq!(resp.len(), 1);
    if let CosmosMsg::Distribution(DistributionMsg::FundCommunityPool { amount }) = &resp[0] {
        assert_eq!(amount, &[Coin::new(10u128.into(), "uluna")]);
    } else {
        panic!("unexpected message");
    }

}

#[test]
fn test_piecewise_linear() {
    let storage = &mut mock_dependencies().storage;
    let payment = Payment::new("vesting");

    let vest = VestInit {
        schedule: Schedule::PiecewiseLinear(vec![
            (1, Uint128::zero()),
            (3, Uint128::new(4)),
            (5, Uint128::new(8)),
        ]),
        total: Uint128::new(8),
        ..Default::default()
    };
    payment.initialize(storage, vest).unwrap();
    payment.set_funded(storage).unwrap();

    let vesting = payment.get_vest(storage).unwrap();

    // just check all the points as there aren't too many.
    assert_eq!(
        payment
            .distributable(storage, &vesting, Timestamp::from_seconds(0))
            .unwrap(),
        Uint128::zero()
    );
    assert_eq!(
        payment
            .distributable(storage, &vesting, Timestamp::from_seconds(1))
            .unwrap(),
        Uint128::zero()
    );
    assert_eq!(
        payment
            .distributable(storage, &vesting, Timestamp::from_seconds(2))
            .unwrap(),
        Uint128::new(2)
    );
    assert_eq!(
        payment
            .distributable(storage, &vesting, Timestamp::from_seconds(3))
            .unwrap(),
        Uint128::new(4)
    );
    assert_eq!(
        payment
            .distributable(storage, &vesting, Timestamp::from_seconds(4))
            .unwrap(),
        Uint128::new(6)
    );
    assert_eq!(
        payment
            .distributable(storage, &vesting, Timestamp::from_seconds(5))
            .unwrap(),
        Uint128::new(8)
    );
    assert_eq!(
        payment
            .distributable(storage, &vesting, Timestamp::from_seconds(6))
            .unwrap(),
        Uint128::new(8)
    );
}
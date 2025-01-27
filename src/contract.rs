use std::env;

use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response,
    StdResult, Uint128,
};
use cw2::set_contract_version;
use cw_ownable::OwnershipError;
use cw_utils::nonpayable;

use crate::denom::CheckedDenom;
use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::PAYMENT;
use crate::payment::{Status, VestInit};

const CONTRACT_NAME: &str = "crates.io:tc-vesting";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {

    // This is a forked cw-vesting contract for Terra Classic
    // where the official team is paid from the Community Pool
    // The workflow will be as follows:
    //
    // 1. instantiate this contract with the vesting details
    // 2. community pool sends tokens to this contract
    // 3. anyone can send `Fund` to mark the contract as funded
    //
    // -> let's make sure the instantiate is non-payable
    nonpayable(&info)?;

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    cw_ownable::initialize_owner(deps.storage, deps.api, Some(&msg.owner.as_str()))?;

    // ensure we are not trying to vest a cw20 token because
    // we can not send cw20 tokens to the community pool
    let denom = msg.denom.into_checked(deps.as_ref())?;
    match denom {
        CheckedDenom::Native(_) => Ok::<(), ContractError>(()),
        CheckedDenom::Cw20 { .. } => Err(ContractError::WrongCw20)?,
    }?;

    let recipient = deps.api.addr_validate(&msg.recipient)?;
    let start_time = msg.start_time.unwrap_or(env.block.time);

    if start_time.plus_seconds(msg.vesting_duration_seconds) <= env.block.time {
        return Err(ContractError::Instavest);
    }

    PAYMENT.initialize(
        deps.storage,
        VestInit {
            total: msg.total,
            schedule: msg.schedule,
            start_time,
            duration_seconds: msg.vesting_duration_seconds,
            denom,
            recipient,
            title: msg.title,
            description: msg.description,
        },
    )?;

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("owner", msg.owner)
    )
}

#[entry_point]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Fund {} => execute_fund(env, deps, info),
        ExecuteMsg::Cancel {} => execute_cancel_vesting_payment(env, deps, info),
        ExecuteMsg::Distribute { amount } => execute_distribute(env, deps, amount),

        // we do not allow updating the ownership - this is a one-way trip
        ExecuteMsg::UpdateOwnership(_msg) => Err(ContractError::Ownable(OwnershipError::NoOwner)),
    }
}

pub fn execute_fund(
    env: Env,
    deps: DepsMut,
    info: MessageInfo,
) -> Result<Response, ContractError> {

    // this is a public function, anyone can call it make
    // sure it is non-payable because the funding comes from
    // a governance spend prop
    nonpayable(&info)?;

    // 1.)  If the contract is already funded, we do nothing
    //      If the contract is canceled, we do nothing
    //      If the contract is unfunded, we continue
    let vest = PAYMENT.get_vest(deps.storage)?;
    match vest.status {
        Status::Unfunded => (),
        Status::Funded => return Err(ContractError::Funded),
        Status::Canceled { .. } => return Err(ContractError::Cancelled),
    };

    // 2.)  Check the token balance of the contract
    let token = vest.clone().denom;
    let balance = token.query_balance(&deps.querier, &env.contract.address)?;
    if balance < vest.total() {
        return Err(ContractError::WrongFundAmount { sent: balance, expected: vest.total() });
    }

    // 3.) if balance is sufficient, we mark the contract as funded
    PAYMENT.set_funded(deps.storage)?;

    Ok(Response::new()
        .add_attribute("method", "fund")
        .add_attribute("from", info.sender))
}

pub fn execute_cancel_vesting_payment(
    env: Env,
    deps: DepsMut,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    cw_ownable::assert_owner(deps.storage, &info.sender)?;
    let total_balance = PAYMENT.get_vest(deps.storage)?.denom.query_balance(&deps.querier, &env.contract.address)?;
    let msgs = PAYMENT.cancel(deps.storage, env.block.time, total_balance)?;

    Ok(Response::new()
        .add_attribute("method", "remove_vesting_payment")
        .add_attribute("owner", info.sender)
        .add_attribute("removed_time", env.block.time.to_string())
        .add_messages(msgs))
}

pub fn execute_distribute(
    env: Env,
    deps: DepsMut,
    request: Option<Uint128>,
) -> Result<Response, ContractError> {
    let msg = PAYMENT.distribute(deps.storage, env.block.time, request)?;

    Ok(Response::new()
        .add_attribute("method", "distribute")
        .add_message(msg))
}

#[entry_point]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Ownership {} => to_json_binary(&cw_ownable::get_ownership(deps.storage)?),
        QueryMsg::Info {} => to_json_binary(&PAYMENT.get_vest(deps.storage)?),
        QueryMsg::Distributable { t } => to_json_binary(&PAYMENT.distributable(
            deps.storage,
            &PAYMENT.get_vest(deps.storage)?,
            t.unwrap_or(env.block.time),
        )?),
        QueryMsg::Vested { t } => to_json_binary(
            &PAYMENT
                .get_vest(deps.storage)?
                .vested(t.unwrap_or(env.block.time)),
        ),
        QueryMsg::TotalToVest {} => to_json_binary(&PAYMENT.get_vest(deps.storage)?.total()),
        QueryMsg::VestDuration {} => to_json_binary(&PAYMENT.duration(deps.storage)?),
    }
}

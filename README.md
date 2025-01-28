## Terra Classic Vesting

This contract is heavily inspired by the Dao-Dao [cw-vesting](https://github.com/DA0-DA0/dao-contracts/tree/development/contracts/external/cw-vesting) contract. However, there are some significant modifications to the original workflow so that this contract is ideally be used to manage governance approved payrolls on the Terra Classic Blockchain (although this software is not restricted to Terra Classic).

### Workflow

At first, this contract is instantiated from the official code ID (**Deployment TBD**). The vesting schedule, vestee, etc. can be chosen at that point using the fields in the `MsgInstantiate`. The owner of this contract should be the Terra Classic governance module account.

After instantiation a Community Pool Spend proposal can be put up requesting a payment into this contract account. If it passes it automatically sends funds to this contract to be able to pay out the vesting schedule. This is all subject to the Terra Governance.

Then, the permissionless `Fund` message can be called by everyone. If the contract balance is sufficient, the contract will be marked as funded and the vesting schedule becomes active. In case the vesting schedule started ***before*** the contract was initially funded by the Community Pool, the respective funds become vested (released) immediately according to the schedule.

The owner (ideally the Terra Classic Governance account) can **cancel the vesting schedule at any point in time**. The funds that have been released up to that point (and are unclaimed) will be immediately sent to the vestee. The rest of the funds (including funding overpayment) will be sent back to the Community Pool.

### Example Instantiate Message

This is an example instantiation message with a 2 months schedule, total vested amount is 1,000,000 LUNC, vested amount will be paid out in 2 equal partions at the end of 30 days periods.

```
{
  "owner": "terra10d07y265gmmuvt4z0w9aw880jnsr700juxf95n",
  "recipient": "terra1...",
  "title": "Payroll for Super-Dooper-Team",
  "description": "This is the payroll contract for the Super-Dooper-Team that is leading this Blockchain to 40bn Market Cap. LFG!",
  "total": "1000000000000",
  "denom": {
    "native": "uluna"
  },
  "schedule": {
    "piecewise_linear": [
      [       1,             "0" ],
      [ 2592000,             "0" ],
      [ 2592001,  "500000000000" ],
      [ 5184000,  "500000000000" ],
      [ 5184001, "1000000000000" ]
    ]
  },
  "start_time": "1738368000000000000",
  "vesting_duration_seconds": 5184001
}
```

### Explanation of Parameters

- `owner`: This should be the Terra Classic Governance account `terra10d07y265gmmuvt4z0w9aw880jnsr700juxf95n`
- `recipient`: This is the vestee's wallet. This is a wallet that should be owned by the Governance approved team. It can be a contract (e.g. internal team management contract), a multisig or a simple wallet.
- `title`: Don't write novels here!
- `description`: Maybe write a bit more...
- `total`: That is the total amount of funds that the vestee will ever be able to withdraw if the payment is not cancelled prematurely. Please note, that the unit of this parameter is `micro`. Meaning, if you want to input 1 LUNC (or USTC), then you need to put `1000000` here.
- `denom`: leave as is (or put `uusd` for the denom, if the payment is made in USTC)
- `schedule`: More about defining payments see below.
- `start_time`: Put the UNIX timestamp (in nanoseconds) when the schedule should start. Calculate the UNIX time from the human-readable date by using [this tool](https://www.unixtimestamp.com/), then multiply the result with `1,000,000,000` and put it into this field.
- `vesting_duration_seconds`: Put the length of the vesting schedule in seconds.

### Definition of Vesting Schedules

The `schedule` parameter defines the vesting schedule. Usually it should be a `piecewise_linear` structure. The `piecewise_linear` consists of an array of pairs, where the first element `t` in each pair defines **seconds into the schedule** and the second element `f(t)` defines the **amount released at that point in time**. The released amount in between the predefined points `t` is linearly interpolated.

Note, that the instantiation message is validated such that the resulting vesting curve `f(t)` is monotonically increasing. Otherwise the instantiation will be rejected. Also note, that the resulting curve is saturating. Meaning, if the definition of `f(t)` exceeds the total amount vested at any point in time the curve will become "flat" starting from that point - no matter what.

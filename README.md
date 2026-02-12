# StreamAgencyEscrow (Claws Network)

Bond + credit-score billing rails for Lobster Lifeguard.

## Economic model

- One-time setup at registration:
  - `setup_fee`: e.g. `200 CLAW`
  - `service_bond`: e.g. `1000 CLAW` minimum
- Promo option:
  - first `N` agents can have setup fee waived (bond still required)
- Service billing:
  - billed once per chain epoch via `billEpoch(agent, epoch, windows)`
- If unpaid after grace period:
  - `enforceEpoch` slashes bond
  - credit score decreases
  - agent can be suspended

## Key endpoints

### Mutable

- `register(metadata, fee_bps, max_windows_per_epoch, max_charge_per_epoch)` payable
- `topUpBond()` payable
- `setBillingGuards(max_windows_per_epoch, max_charge_per_epoch)`
- `pause()`
- `resumeIfHealthy()`
- `cancelAndWithdraw()`
- `billEpoch(agent, epoch, windows)` operator-only
- `settleEpoch(epoch)` payable (agent)
- `enforceEpoch(agent, epoch)`
- `withdrawOwner(amount, to)` owner-only
- `setOwner(new_owner)` owner-only
- `setOperator(new_operator)` owner-only
- `setWindowReward(window_reward)` owner-only
- `setPromoSlots(slots)` owner-only
- `setMaxBackbillEpochs(value)` owner-only
- `setHardMaxWindowsPerEpoch(value)` owner-only

### Views

- `getAgentInfo(agent)`
- `getAgentFinancials(agent)`
- `getEpochDebt(agent, epoch)`
- `getEpochState(agent, epoch)`
- `getClaimableOwner()`
- `getConfig()`
- `getPromoUsage()`
- `getActiveAgentCount()`

## Build

```bash
cd /Users/ls/Documents/Claws\ Network/stream-escrow
sc-meta all build
```

Outputs:
- `output/stream-escrow.wasm`
- `output/stream-escrow.abi.json`

## Deploy

```bash
cd /Users/ls/Documents/Claws\ Network/stream-escrow/cli
python3 escrow_utils.py deploy \
  --pem /path/to/owner.pem \
  --operator claw1operator... \
  --window-reward-atto 1000000000000000000 \
  --setup-fee-atto 200000000000000000000 \
  --min-bond-atto 1000000000000000000000 \
  --promo-free-slots 100 \
  --grace-epochs 1 \
  --max-backbill-epochs 2 \
  --hard-max-windows-per-epoch 48
```

## Agent registration

Normal registration (200 + 1000 = 1200 CLAW):

```bash
python3 escrow_utils.py register \
  --contract claw1contract... \
  --pem /path/to/agent.pem \
  --metadata str:lobster-lifeguard-v2 \
  --fee-bps 500 \
  --max-windows-per-epoch 48 \
  --max-charge-per-epoch-atto 50000000000000000000 \
  --deposit-atto 1200000000000000000000
```

If promo slot applies, deposit can be `1000 CLAW`.

Agent can later tighten caps:

```bash
python3 escrow_utils.py set-billing-guards \
  --contract claw1contract... \
  --pem /path/to/agent.pem \
  --max-windows-per-epoch 24 \
  --max-charge-per-epoch-atto 25000000000000000000
```

## Operations

Bill a closed epoch:

```bash
python3 escrow_utils.py bill-epoch \
  --contract claw1contract... \
  --pem /path/to/operator.pem \
  --agent claw1agent... \
  --epoch 1234 \
  --windows 12
```

Agent settlement:

```bash
python3 escrow_utils.py settle-epoch \
  --contract claw1contract... \
  --pem /path/to/agent.pem \
  --epoch 1234 \
  --amount-atto 100000000000000000
```

Enforce non-payment (post-grace):

```bash
python3 escrow_utils.py enforce-epoch \
  --contract claw1contract... \
  --pem /path/to/operator.pem \
  --agent claw1agent... \
  --epoch 1234
```

Owner anti-overbilling caps:

```bash
python3 escrow_utils.py set-max-backbill-epochs --contract claw1contract... --pem /path/to/owner.pem --value 2
python3 escrow_utils.py set-hard-max-windows --contract claw1contract... --pem /path/to/owner.pem --value 48
```

Owner disbursement:

```bash
python3 escrow_utils.py withdraw-owner \
  --contract claw1contract... \
  --pem /path/to/owner.pem \
  --amount-atto 500000000000000000 \
  --to claw1treasury...
```

## Frontend funnel

`frontend/index.html` is a static, single-file ad-style funnel for humans:
- value proposition
- live contract snapshot
- generated signup/operator commands

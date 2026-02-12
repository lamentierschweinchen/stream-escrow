# Deploy Stream Escrow From GitHub (Agent Runbook)

Use this exact sequence on the machine where your on-chain agent runs.

## 0) Inputs you must have

- A funded deployer wallet PEM file on that machine (this wallet becomes contract owner).
- An operator address (the agency operator address, `claw1...`).

## 1) Install system dependencies

```bash
sudo apt-get update
sudo apt-get install -y curl git python3 python3-pip pipx build-essential pkg-config libssl-dev
pipx ensurepath
```

Open a new shell after `pipx ensurepath`.

## 2) Install Claws + Rust toolchain

```bash
pipx install claw-sdk-cli
curl https://sh.rustup.rs -sSf | sh -s -- -y
source "$HOME/.cargo/env"
rustup default 1.86.0
rustup target add wasm32-unknown-unknown
cargo install multiversx-sc-meta --locked --version 0.54.6
```

Note: this project uses `multiversx-sc` 0.54.x. Rust `1.86.0` avoids version compatibility issues.

## 3) Clone the repository

```bash
git clone https://github.com/lamentierschweinchen/stream-escrow.git
cd stream-escrow
git checkout main
git pull --ff-only
```

Optional: lock to the known good commit used here.

```bash
git checkout a3c312a
```

## 4) Build contract artifacts

```bash
cargo check
sc-meta all build
```

Expected artifacts:
- `output/stream-escrow.wasm`
- `output/stream-escrow.abi.json`

## 5) Set deployment variables

```bash
export OWNER_PEM="/absolute/path/to/owner.pem"
export OPERATOR_ADDR="claw1replace_with_operator_address"
```

## 6) Deploy contract

From repo root (`stream-escrow/`):

```bash
python3 cli/escrow_utils.py deploy \
  --pem "$OWNER_PEM" \
  --operator "$OPERATOR_ADDR" \
  --window-reward-atto 1000000000000000000 \
  --setup-fee-atto 200000000000000000000 \
  --min-bond-atto 1000000000000000000000 \
  --promo-free-slots 100 \
  --grace-epochs 1 \
  --max-backbill-epochs 2 \
  --hard-max-windows-per-epoch 48
```

Save the deployed contract address from command output (`claw1...`).

## 7) Verify deployment immediately

```bash
python3 cli/escrow_utils.py query --contract "<DEPLOYED_CONTRACT>" --function getConfig
python3 cli/escrow_utils.py query --contract "<DEPLOYED_CONTRACT>" --function getServiceStats
```

If both queries return successfully, deployment is live.

## 8) (Optional but recommended) set default contract in CLI config

Edit `cli/config.py` and set:

```python
CONTRACT_ADDRESS = "<DEPLOYED_CONTRACT>"
```

Then future CLI commands can omit `--contract`.

## 9) Smoke test a registration path

Using a funded test agent PEM:

```bash
python3 cli/escrow_utils.py register \
  --contract "<DEPLOYED_CONTRACT>" \
  --pem /absolute/path/to/test-agent.pem \
  --metadata str:lobster-lifeguard-v2 \
  --fee-bps 500 \
  --max-windows-per-epoch 48 \
  --max-charge-per-epoch-atto 50000000000000000000 \
  --deposit-atto 1200000000000000000000
```

Then verify:

```bash
python3 cli/escrow_utils.py query --contract "<DEPLOYED_CONTRACT>" --function getAgentInfo --arguments <TEST_AGENT_ADDRESS>
```

## 10) Post-deploy values to publish

- Contract address (`claw1...`)
- ABI path in repo: `abi/stream-escrow.abi.json`
- Frontend URL: `https://frontend-three-pi-52.vercel.app?contract=<DEPLOYED_CONTRACT>`

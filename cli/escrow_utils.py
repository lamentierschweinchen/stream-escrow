#!/usr/bin/env python3
import argparse
import json
import subprocess
import sys
from typing import List

from config import (
    BYTECODE_PATH,
    CHAIN_ID,
    CONTRACT_ADDRESS,
    GAS_LIMIT_CALL,
    GAS_LIMIT_DEPLOY,
    GAS_PRICE,
    PROXY_URL,
)


def run(cmd: List[str]) -> subprocess.CompletedProcess:
    proc = subprocess.run(cmd, capture_output=True, text=True)
    if proc.returncode != 0:
        raise RuntimeError(
            f"Command failed ({proc.returncode}): {' '.join(cmd)}\n"
            f"stdout:\n{proc.stdout}\n"
            f"stderr:\n{proc.stderr}"
        )
    return proc


def deploy(
    pem: str,
    operator: str,
    window_reward_atto: str,
    setup_fee_atto: str,
    min_bond_atto: str,
    promo_free_slots: int,
    grace_epochs: int,
    max_backbill_epochs: int,
    hard_max_windows_per_epoch: int,
) -> None:
    cmd = [
        "clawpy",
        "contract",
        "deploy",
        f"--bytecode={BYTECODE_PATH}",
        f"--proxy={PROXY_URL}",
        f"--chain={CHAIN_ID}",
        f"--gas-limit={GAS_LIMIT_DEPLOY}",
        f"--gas-price={GAS_PRICE}",
        f"--pem={pem}",
        "--arguments",
        operator,
        window_reward_atto,
        setup_fee_atto,
        min_bond_atto,
        str(promo_free_slots),
        str(grace_epochs),
        str(max_backbill_epochs),
        str(hard_max_windows_per_epoch),
        "--send",
    ]
    out = run(cmd)
    print(out.stdout)


def call(
    pem: str,
    function: str,
    arguments: List[str],
    value: str | None = None,
    contract: str | None = None,
) -> None:
    address = contract or CONTRACT_ADDRESS
    if not address:
        raise RuntimeError("Contract address missing. Set cli/config.py CONTRACT_ADDRESS or pass --contract")

    cmd = [
        "clawpy",
        "contract",
        "call",
        address,
        "--function",
        function,
        "--gas-limit",
        str(GAS_LIMIT_CALL),
        "--gas-price",
        str(GAS_PRICE),
        "--pem",
        pem,
        "--chain",
        CHAIN_ID,
        "--proxy",
        PROXY_URL,
    ]

    if value:
        cmd.extend(["--value", value])

    if arguments:
        cmd.append("--arguments")
        cmd.extend(arguments)

    cmd.append("--send")
    out = run(cmd)
    print(out.stdout)


def query(function: str, arguments: List[str], contract: str | None = None) -> None:
    address = contract or CONTRACT_ADDRESS
    if not address:
        raise RuntimeError("Contract address missing. Set cli/config.py CONTRACT_ADDRESS or pass --contract")

    cmd = [
        "clawpy",
        "contract",
        "query",
        address,
        "--function",
        function,
        "--proxy",
        PROXY_URL,
    ]

    if arguments:
        cmd.append("--arguments")
        cmd.extend(arguments)

    out = run(cmd)
    try:
        parsed = json.loads(out.stdout)
        print(json.dumps(parsed, indent=2))
    except json.JSONDecodeError:
        print(out.stdout)


def main() -> int:
    parser = argparse.ArgumentParser(description="StreamAgencyEscrow clawpy helpers")
    sub = parser.add_subparsers(dest="cmd", required=True)

    p_deploy = sub.add_parser("deploy")
    p_deploy.add_argument("--pem", required=True)
    p_deploy.add_argument("--operator", required=True)
    p_deploy.add_argument("--window-reward-atto", required=True)
    p_deploy.add_argument("--setup-fee-atto", default="200000000000000000000")
    p_deploy.add_argument("--min-bond-atto", default="1000000000000000000000")
    p_deploy.add_argument("--promo-free-slots", type=int, default=100)
    p_deploy.add_argument("--grace-epochs", type=int, default=1)
    p_deploy.add_argument("--max-backbill-epochs", type=int, default=2)
    p_deploy.add_argument("--hard-max-windows-per-epoch", type=int, default=48)

    p_register = sub.add_parser("register")
    p_register.add_argument("--pem", required=True)
    p_register.add_argument("--metadata", default="str:lobster-lifeguard-v2")
    p_register.add_argument("--fee-bps", type=int, default=500)
    p_register.add_argument("--max-windows-per-epoch", type=int, default=48)
    p_register.add_argument("--max-charge-per-epoch-atto", default="50000000000000000000")
    p_register.add_argument("--deposit-atto", required=True)
    p_register.add_argument("--contract", default="")

    p_set_guards = sub.add_parser("set-billing-guards")
    p_set_guards.add_argument("--pem", required=True)
    p_set_guards.add_argument("--max-windows-per-epoch", type=int, required=True)
    p_set_guards.add_argument("--max-charge-per-epoch-atto", required=True)
    p_set_guards.add_argument("--contract", default="")

    p_topup = sub.add_parser("topup-bond")
    p_topup.add_argument("--pem", required=True)
    p_topup.add_argument("--amount-atto", required=True)
    p_topup.add_argument("--contract", default="")

    p_pause = sub.add_parser("pause")
    p_pause.add_argument("--pem", required=True)
    p_pause.add_argument("--contract", default="")

    p_resume = sub.add_parser("resume-healthy")
    p_resume.add_argument("--pem", required=True)
    p_resume.add_argument("--contract", default="")

    p_cancel = sub.add_parser("cancel")
    p_cancel.add_argument("--pem", required=True)
    p_cancel.add_argument("--contract", default="")

    p_bill = sub.add_parser("bill-epoch")
    p_bill.add_argument("--pem", required=True, help="Operator PEM")
    p_bill.add_argument("--agent", required=True)
    p_bill.add_argument("--epoch", type=int, required=True)
    p_bill.add_argument("--windows", type=int, required=True)
    p_bill.add_argument("--contract", default="")

    p_settle = sub.add_parser("settle-epoch")
    p_settle.add_argument("--pem", required=True)
    p_settle.add_argument("--epoch", type=int, required=True)
    p_settle.add_argument("--amount-atto", required=True)
    p_settle.add_argument("--contract", default="")

    p_enforce = sub.add_parser("enforce-epoch")
    p_enforce.add_argument("--pem", required=True)
    p_enforce.add_argument("--agent", required=True)
    p_enforce.add_argument("--epoch", type=int, required=True)
    p_enforce.add_argument("--contract", default="")

    p_withdraw = sub.add_parser("withdraw-owner")
    p_withdraw.add_argument("--pem", required=True)
    p_withdraw.add_argument("--amount-atto", required=True)
    p_withdraw.add_argument("--to", required=True)
    p_withdraw.add_argument("--contract", default="")

    p_query = sub.add_parser("query")
    p_query.add_argument("--function", required=True)
    p_query.add_argument("--arguments", nargs="*", default=[])
    p_query.add_argument("--contract", default="")

    p_set_backbill = sub.add_parser("set-max-backbill-epochs")
    p_set_backbill.add_argument("--pem", required=True)
    p_set_backbill.add_argument("--value", type=int, required=True)
    p_set_backbill.add_argument("--contract", default="")

    p_set_hard = sub.add_parser("set-hard-max-windows")
    p_set_hard.add_argument("--pem", required=True)
    p_set_hard.add_argument("--value", type=int, required=True)
    p_set_hard.add_argument("--contract", default="")

    args = parser.parse_args()

    if args.cmd == "deploy":
        deploy(
            args.pem,
            args.operator,
            args.window_reward_atto,
            args.setup_fee_atto,
            args.min_bond_atto,
            args.promo_free_slots,
            args.grace_epochs,
            args.max_backbill_epochs,
            args.hard_max_windows_per_epoch,
        )
        return 0

    if args.cmd == "register":
        call(
            pem=args.pem,
            function="register",
            arguments=[
                args.metadata,
                str(args.fee_bps),
                str(args.max_windows_per_epoch),
                args.max_charge_per_epoch_atto,
            ],
            value=args.deposit_atto,
            contract=args.contract or None,
        )
        return 0

    if args.cmd == "set-billing-guards":
        call(
            pem=args.pem,
            function="setBillingGuards",
            arguments=[str(args.max_windows_per_epoch), args.max_charge_per_epoch_atto],
            contract=args.contract or None,
        )
        return 0

    if args.cmd == "topup-bond":
        call(
            pem=args.pem,
            function="topUpBond",
            arguments=[],
            value=args.amount_atto,
            contract=args.contract or None,
        )
        return 0

    if args.cmd == "pause":
        call(pem=args.pem, function="pause", arguments=[], contract=args.contract or None)
        return 0

    if args.cmd == "resume-healthy":
        call(pem=args.pem, function="resumeIfHealthy", arguments=[], contract=args.contract or None)
        return 0

    if args.cmd == "cancel":
        call(pem=args.pem, function="cancelAndWithdraw", arguments=[], contract=args.contract or None)
        return 0

    if args.cmd == "bill-epoch":
        call(
            pem=args.pem,
            function="billEpoch",
            arguments=[args.agent, str(args.epoch), str(args.windows)],
            contract=args.contract or None,
        )
        return 0

    if args.cmd == "settle-epoch":
        call(
            pem=args.pem,
            function="settleEpoch",
            arguments=[str(args.epoch)],
            value=args.amount_atto,
            contract=args.contract or None,
        )
        return 0

    if args.cmd == "enforce-epoch":
        call(
            pem=args.pem,
            function="enforceEpoch",
            arguments=[args.agent, str(args.epoch)],
            contract=args.contract or None,
        )
        return 0

    if args.cmd == "withdraw-owner":
        call(
            pem=args.pem,
            function="withdrawOwner",
            arguments=[args.amount_atto, args.to],
            contract=args.contract or None,
        )
        return 0

    if args.cmd == "query":
        query(args.function, args.arguments, contract=args.contract or None)
        return 0

    if args.cmd == "set-max-backbill-epochs":
        call(
            pem=args.pem,
            function="setMaxBackbillEpochs",
            arguments=[str(args.value)],
            contract=args.contract or None,
        )
        return 0

    if args.cmd == "set-hard-max-windows":
        call(
            pem=args.pem,
            function="setHardMaxWindowsPerEpoch",
            arguments=[str(args.value)],
            contract=args.contract or None,
        )
        return 0

    return 1


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        raise

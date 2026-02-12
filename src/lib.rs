#![no_std]

multiversx_sc::imports!();
multiversx_sc::derive_imports!();

pub mod types;

use types::{AgentInfo, AgentStatus, EpochState};

const BPS_DENOMINATOR: u64 = 10_000;

const DEFAULT_INITIAL_CREDIT: u64 = 700;
const MAX_CREDIT: u64 = 1_000;
const MIN_ACTIVE_CREDIT: u64 = 500;

const SCORE_BONUS_ON_TIME: u64 = 5;
const SCORE_PENALTY_LATE: u64 = 15;
const SCORE_PENALTY_SLASHED: u64 = 60;
const SCORE_PENALTY_DELINQUENT: u64 = 90;

const STATUS_ACTIVE: u64 = 1;
const STATUS_PAUSED: u64 = 2;
const STATUS_SUSPENDED: u64 = 3;
const STATUS_CANCELLED: u64 = 4;

#[multiversx_sc::contract]
pub trait StreamAgencyEscrow {
    #[init]
    fn init(
        &self,
        operator: ManagedAddress,
        window_reward: BigUint,
        setup_fee: BigUint,
        min_bond: BigUint,
        promo_free_slots: u64,
        grace_epochs: u64,
        max_backbill_epochs: u64,
        hard_max_windows_per_epoch: u64,
    ) {
        require!(!operator.is_zero(), "Invalid operator");
        require!(window_reward > 0u64, "Window reward must be positive");
        require!(setup_fee > 0u64, "Setup fee must be positive");
        require!(min_bond > 0u64, "Min bond must be positive");
        require!(max_backbill_epochs > 0u64, "Invalid backbill limit");
        require!(hard_max_windows_per_epoch > 0u64, "Invalid window hard cap");

        let owner = self.blockchain().get_caller();
        self.owner().set(&owner);
        self.operator().set(&operator);

        self.window_reward().set(&window_reward);
        self.setup_fee().set(&setup_fee);
        self.min_bond().set(&min_bond);

        self.promo_free_slots().set(promo_free_slots);
        self.promo_used().set(0u64);

        self.grace_epochs().set(grace_epochs);
        self.max_backbill_epochs().set(max_backbill_epochs);
        self.hard_max_windows_per_epoch()
            .set(hard_max_windows_per_epoch);

        self.active_agent_count().set(0u64);
        self.claimable_owner().set(BigUint::zero());
    }

    #[upgrade]
    fn upgrade(&self) {}

    #[endpoint(register)]
    #[payable("EGLD")]
    fn register(
        &self,
        metadata: ManagedBuffer,
        fee_bps: u64,
        max_windows_per_epoch: u64,
        max_charge_per_epoch: BigUint,
    ) {
        require!(fee_bps > 0 && fee_bps <= BPS_DENOMINATOR, "Invalid fee bps");
        require!(max_windows_per_epoch > 0, "Invalid max windows");
        require!(
            max_windows_per_epoch <= self.hard_max_windows_per_epoch().get(),
            "Max windows exceeds hard cap"
        );
        require!(max_charge_per_epoch > 0u64, "Invalid max epoch charge");

        let caller = self.blockchain().get_caller();
        let payment = self.call_value().egld_value().clone_value();
        require!(payment > 0u64, "Registration requires payment");

        let now_epoch = self.blockchain().get_block_epoch();

        if self.agent_info(&caller).is_empty() {
            let setup_fee = self.setup_fee().get();
            let min_bond = self.min_bond().get();

            let promo_available = self.promo_used().get() < self.promo_free_slots().get();
            let mut charged_setup_fee = setup_fee.clone();
            let mut used_promo = false;

            if promo_available {
                charged_setup_fee = BigUint::zero();
                used_promo = true;
                self.promo_used().update(|used| *used += 1u64);
            }

            let required_total = &charged_setup_fee + &min_bond;
            require!(payment >= required_total, "Insufficient register payment");

            if charged_setup_fee > 0u64 {
                self.claimable_owner().update(|v| *v += &charged_setup_fee);
            }

            let bond_add = payment - &charged_setup_fee;
            self.bond_balance(&caller).set(&bond_add);
            self.outstanding_total(&caller).set(BigUint::zero());

            let info = AgentInfo {
                fee_bps,
                max_windows_per_epoch,
                max_charge_per_epoch,
                credit_score: DEFAULT_INITIAL_CREDIT,
                status: AgentStatus::Active,
                used_promo,
                joined_epoch: now_epoch,
                last_billed_epoch: now_epoch.saturating_sub(1),
                metadata,
            };
            self.agent_info(&caller).set(&info);
            self.active_agent_count().update(|count| *count += 1u64);

            self.registered_event(&caller, fee_bps, max_windows_per_epoch, &bond_add);
            return;
        }

        // Existing agent update/reactivation.
        let mut info = self.agent_info(&caller).get();
        let mut was_active = info.status == AgentStatus::Active;

        if info.status == AgentStatus::Cancelled {
            require!(self.outstanding_total(&caller).get() == 0u64, "Outstanding debt exists");
            require!(payment >= self.min_bond().get(), "Need min bond to reactivate");
            info.status = AgentStatus::Suspended;
            info.last_billed_epoch = now_epoch.saturating_sub(1);
            was_active = false;
        }

        info.fee_bps = fee_bps;
        info.max_windows_per_epoch = max_windows_per_epoch;
        info.max_charge_per_epoch = max_charge_per_epoch;
        info.metadata = metadata;
        self.agent_info(&caller).set(&info);

        self.bond_balance(&caller).update(|v| *v += &payment);

        if !was_active && self.can_be_active(&caller) {
            self.set_status(&caller, AgentStatus::Active);
        }

        self.bond_topped_up_event(&caller, &payment);
    }

    #[endpoint(topUpBond)]
    #[payable("EGLD")]
    fn top_up_bond(&self) {
        let caller = self.blockchain().get_caller();
        self.require_agent_exists(&caller);

        let payment = self.call_value().egld_value().clone_value();
        require!(payment > 0u64, "Top-up requires payment");

        let info = self.agent_info(&caller).get();
        require!(info.status != AgentStatus::Cancelled, "Agent cancelled");

        self.bond_balance(&caller).update(|v| *v += &payment);
        self.bond_topped_up_event(&caller, &payment);
    }

    #[endpoint(setBillingGuards)]
    fn set_billing_guards(&self, max_windows_per_epoch: u64, max_charge_per_epoch: BigUint) {
        let caller = self.blockchain().get_caller();
        self.require_agent_exists(&caller);

        require!(max_windows_per_epoch > 0, "Invalid max windows");
        require!(
            max_windows_per_epoch <= self.hard_max_windows_per_epoch().get(),
            "Max windows exceeds hard cap"
        );
        require!(max_charge_per_epoch > 0u64, "Invalid max epoch charge");

        let mut info = self.agent_info(&caller).get();
        require!(info.status != AgentStatus::Cancelled, "Agent cancelled");
        info.max_windows_per_epoch = max_windows_per_epoch;
        info.max_charge_per_epoch = max_charge_per_epoch;
        self.agent_info(&caller).set(&info);
    }

    #[endpoint(pause)]
    fn pause(&self) {
        let caller = self.blockchain().get_caller();
        self.require_agent_exists(&caller);

        let info = self.agent_info(&caller).get();
        require!(info.status == AgentStatus::Active, "Not active");

        self.set_status(&caller, AgentStatus::Paused);
    }

    #[endpoint(resumeIfHealthy)]
    fn resume_if_healthy(&self) {
        let caller = self.blockchain().get_caller();
        self.require_agent_exists(&caller);

        let info = self.agent_info(&caller).get();
        require!(
            info.status == AgentStatus::Paused || info.status == AgentStatus::Suspended,
            "Not paused or suspended"
        );
        require!(self.can_be_active(&caller), "Health checks failed");

        self.set_status(&caller, AgentStatus::Active);
    }

    #[endpoint(cancelAndWithdraw)]
    fn cancel_and_withdraw(&self) {
        let caller = self.blockchain().get_caller();
        self.require_agent_exists(&caller);

        let current_status = self.agent_info(&caller).get().status;
        if current_status != AgentStatus::Cancelled {
            self.set_status(&caller, AgentStatus::Cancelled);
        }

        // First, attempt to satisfy outstanding debt from bond.
        let debt = self.outstanding_total(&caller).get();
        if debt > 0u64 {
            let bond = self.bond_balance(&caller).get();
            let slash = self.min_biguint(&debt, &bond);
            if slash > 0u64 {
                self.bond_balance(&caller).set(&(bond - &slash));
                self.outstanding_total(&caller).set(&(debt - &slash));
                self.claimable_owner().update(|v| *v += &slash);
            }
        }

        let payout = self.bond_balance(&caller).get();
        if payout > 0u64 {
            self.bond_balance(&caller).clear();
            self.send().direct_egld(&caller, &payout);
        }

        self.cancelled_event(&caller, &payout);
    }

    #[endpoint(billEpoch)]
    fn bill_epoch(&self, agent: ManagedAddress, epoch: u64, windows: u64) -> BigUint {
        self.only_operator();
        self.require_agent_exists(&agent);

        require!(windows > 0, "Windows must be positive");

        let current_epoch = self.blockchain().get_block_epoch();
        require!(epoch < current_epoch, "Epoch not closed yet");
        require!(
            current_epoch - epoch <= self.max_backbill_epochs().get(),
            "Epoch too old to bill"
        );

        let info = self.agent_info(&agent).get();
        require!(info.status != AgentStatus::Cancelled, "Agent cancelled");
        require!(epoch >= info.joined_epoch, "Cannot bill before join epoch");
        require!(epoch > info.last_billed_epoch, "Epoch already passed in billing order");
        require!(
            windows <= info.max_windows_per_epoch,
            "Exceeds max windows per epoch"
        );
        require!(
            windows <= self.hard_max_windows_per_epoch().get(),
            "Exceeds global windows hard cap"
        );
        require!(
            self.epoch_state(&agent, epoch).is_empty(),
            "Epoch already billed"
        );

        let due = self.compute_fee_amount(windows, info.fee_bps);
        require!(
            due <= info.max_charge_per_epoch,
            "Exceeds agent max charge per epoch"
        );

        self.epoch_windows(&agent, epoch).set(windows);
        self.epoch_due(&agent, epoch).set(&due);
        self.epoch_deadline(&agent, epoch)
            .set(epoch + self.grace_epochs().get());
        self.epoch_state(&agent, epoch).set(EpochState::Billed);
        self.epoch_score_applied(&agent, epoch).set(false);

        self.outstanding_total(&agent).update(|v| *v += &due);

        let mut info_mut = info;
        if epoch > info_mut.last_billed_epoch {
            info_mut.last_billed_epoch = epoch;
            self.agent_info(&agent).set(&info_mut);
        }

        self.epoch_billed_event(&agent, epoch, windows, &due);
        due
    }

    #[endpoint(settleEpoch)]
    #[payable("EGLD")]
    fn settle_epoch(&self, epoch: u64) {
        let caller = self.blockchain().get_caller();
        self.require_agent_exists(&caller);

        require!(!self.epoch_due(&caller, epoch).is_empty(), "Epoch not billed");

        let due = self.epoch_due(&caller, epoch).get();
        require!(due > 0u64, "Epoch already settled");

        let payment = self.call_value().egld_value().clone_value();
        require!(payment > 0u64, "Payment required");

        let applied = self.min_biguint(&payment, &due);
        let remaining = &due - &applied;

        self.epoch_due(&caller, epoch).set(&remaining);
        self.outstanding_total(&caller).update(|v| *v -= &applied);
        self.claimable_owner().update(|v| *v += &applied);

        let extra = payment - &applied;
        if extra > 0u64 {
            self.bond_balance(&caller).update(|v| *v += &extra);
        }

        if remaining == 0u64 {
            let current_epoch = self.blockchain().get_block_epoch();
            let deadline = self.epoch_deadline(&caller, epoch).get();
            if !self.epoch_score_applied(&caller, epoch).get() {
                if current_epoch <= deadline {
                    self.apply_credit_delta(&caller, SCORE_BONUS_ON_TIME as i64);
                    self.epoch_state(&caller, epoch).set(EpochState::SettledOnTime);
                } else {
                    self.apply_credit_delta(&caller, -(SCORE_PENALTY_LATE as i64));
                    self.epoch_state(&caller, epoch).set(EpochState::SettledLate);
                }
                self.epoch_score_applied(&caller, epoch).set(true);
            }
        }

        if self.agent_info(&caller).get().status != AgentStatus::Cancelled && self.can_be_active(&caller) {
            self.set_status(&caller, AgentStatus::Active);
        }

        self.epoch_settled_event(&caller, epoch, &applied);
    }

    #[endpoint(enforceEpoch)]
    fn enforce_epoch(&self, agent: ManagedAddress, epoch: u64) {
        self.require_agent_exists(&agent);
        require!(!self.epoch_due(&agent, epoch).is_empty(), "Epoch not billed");

        let current_epoch = self.blockchain().get_block_epoch();
        let deadline = self.epoch_deadline(&agent, epoch).get();
        require!(current_epoch > deadline, "Still in grace period");

        let due = self.epoch_due(&agent, epoch).get();
        require!(due > 0u64, "Nothing due");

        let bond = self.bond_balance(&agent).get();
        let slash = self.min_biguint(&due, &bond);

        if slash > 0u64 {
            self.bond_balance(&agent).set(&(bond - &slash));
            self.epoch_due(&agent, epoch).set(&(due - &slash));
            self.outstanding_total(&agent).update(|v| *v -= &slash);
            self.claimable_owner().update(|v| *v += &slash);
        }

        let remaining_after = self.epoch_due(&agent, epoch).get();

        if !self.epoch_score_applied(&agent, epoch).get() {
            if remaining_after == 0u64 {
                self.apply_credit_delta(&agent, -(SCORE_PENALTY_SLASHED as i64));
                self.epoch_state(&agent, epoch).set(EpochState::Slashed);
            } else {
                self.apply_credit_delta(&agent, -(SCORE_PENALTY_DELINQUENT as i64));
                self.epoch_state(&agent, epoch).set(EpochState::Delinquent);
            }
            self.epoch_score_applied(&agent, epoch).set(true);
        }

        if !self.can_be_active(&agent) {
            self.set_status(&agent, AgentStatus::Suspended);
        }

        self.epoch_enforced_event(&agent, epoch, &slash);
    }

    #[endpoint(withdrawOwner)]
    fn withdraw_owner(&self, amount: BigUint, to: ManagedAddress) {
        self.only_owner();
        require!(amount > 0u64, "Amount must be positive");
        require!(!to.is_zero(), "Invalid recipient");

        let claimable = self.claimable_owner().get();
        require!(claimable >= amount, "Insufficient claimable");

        self.claimable_owner().set(&(claimable - &amount));
        self.send().direct_egld(&to, &amount);

        self.owner_withdrawn_event(&to, &amount);
    }

    #[endpoint(setOperator)]
    fn set_operator(&self, new_operator: ManagedAddress) {
        self.only_owner();
        require!(!new_operator.is_zero(), "Invalid operator");
        self.operator().set(&new_operator);
    }

    #[endpoint(setOwner)]
    fn set_owner(&self, new_owner: ManagedAddress) {
        self.only_owner();
        require!(!new_owner.is_zero(), "Invalid owner");
        self.owner().set(&new_owner);
    }

    #[endpoint(setWindowReward)]
    fn set_window_reward(&self, window_reward: BigUint) {
        self.only_owner();
        require!(window_reward > 0u64, "Window reward must be positive");
        self.window_reward().set(&window_reward);
    }

    #[endpoint(setPromoSlots)]
    fn set_promo_slots(&self, promo_free_slots: u64) {
        self.only_owner();
        self.promo_free_slots().set(promo_free_slots);
    }

    #[endpoint(setMaxBackbillEpochs)]
    fn set_max_backbill_epochs(&self, max_backbill_epochs: u64) {
        self.only_owner();
        require!(max_backbill_epochs > 0u64, "Invalid backbill limit");
        self.max_backbill_epochs().set(max_backbill_epochs);
    }

    #[endpoint(setHardMaxWindowsPerEpoch)]
    fn set_hard_max_windows_per_epoch(&self, hard_max_windows_per_epoch: u64) {
        self.only_owner();
        require!(hard_max_windows_per_epoch > 0u64, "Invalid window hard cap");
        self.hard_max_windows_per_epoch()
            .set(hard_max_windows_per_epoch);
    }

    #[view(getAgentInfo)]
    fn get_agent_info_view(&self, agent: ManagedAddress) -> OptionalValue<AgentInfo<Self::Api>> {
        if self.agent_info(&agent).is_empty() {
            return OptionalValue::None;
        }
        OptionalValue::Some(self.agent_info(&agent).get())
    }

    #[view(getAgentFinancials)]
    fn get_agent_financials_view(
        &self,
        agent: ManagedAddress,
    ) -> MultiValue2<BigUint, BigUint> {
        (self.bond_balance(&agent).get(), self.outstanding_total(&agent).get()).into()
    }

    #[view(getEpochDebt)]
    fn get_epoch_debt_view(&self, agent: ManagedAddress, epoch: u64) -> BigUint {
        self.epoch_due(&agent, epoch).get()
    }

    #[view(getEpochState)]
    fn get_epoch_state_view(&self, agent: ManagedAddress, epoch: u64) -> OptionalValue<EpochState> {
        if self.epoch_state(&agent, epoch).is_empty() {
            return OptionalValue::None;
        }
        OptionalValue::Some(self.epoch_state(&agent, epoch).get())
    }

    #[view(getClaimableOwner)]
    fn get_claimable_owner_view(&self) -> BigUint {
        self.claimable_owner().get()
    }

    #[view(getConfig)]
    fn get_config_view(
        &self,
    ) -> MultiValue9<ManagedAddress, ManagedAddress, BigUint, BigUint, BigUint, u64, u64, u64, u64>
    {
        (
            self.owner().get(),
            self.operator().get(),
            self.window_reward().get(),
            self.setup_fee().get(),
            self.min_bond().get(),
            self.promo_free_slots().get(),
            self.grace_epochs().get(),
            self.max_backbill_epochs().get(),
            self.hard_max_windows_per_epoch().get(),
        )
            .into()
    }

    #[view(getPromoUsage)]
    fn get_promo_usage_view(&self) -> MultiValue2<u64, u64> {
        (self.promo_used().get(), self.promo_free_slots().get()).into()
    }

    #[view(getActiveAgentCount)]
    fn get_active_agent_count_view(&self) -> u64 {
        self.active_agent_count().get()
    }

    fn compute_fee_amount(&self, windows: u64, fee_bps: u64) -> BigUint {
        let mut fee = self.window_reward().get();
        fee *= windows;
        fee *= fee_bps;
        fee /= BPS_DENOMINATOR;
        require!(fee > 0u64, "Fee rounds to zero");
        fee
    }

    fn apply_credit_delta(&self, agent: &ManagedAddress, delta: i64) {
        let mut info = self.agent_info(agent).get();
        if delta >= 0 {
            let add = delta as u64;
            let next = info.credit_score.saturating_add(add);
            info.credit_score = core::cmp::min(next, MAX_CREDIT);
        } else {
            let sub = (-delta) as u64;
            info.credit_score = info.credit_score.saturating_sub(sub);
        }
        self.agent_info(agent).set(&info);
    }

    fn can_be_active(&self, agent: &ManagedAddress) -> bool {
        if self.agent_info(agent).is_empty() {
            return false;
        }
        let info = self.agent_info(agent).get();
        if info.status == AgentStatus::Cancelled {
            return false;
        }
        if info.credit_score < MIN_ACTIVE_CREDIT {
            return false;
        }
        if self.bond_balance(agent).get() < self.min_bond().get() {
            return false;
        }
        if self.outstanding_total(agent).get() > 0u64 {
            return false;
        }
        true
    }

    fn set_status(&self, agent: &ManagedAddress, next: AgentStatus) {
        let mut info = self.agent_info(agent).get();
        let prev = info.status.clone();
        if prev == next {
            return;
        }

        if prev == AgentStatus::Active {
            let count = self.active_agent_count().get();
            if count > 0u64 {
                self.active_agent_count().set(count - 1u64);
            }
        }

        if next == AgentStatus::Active {
            self.active_agent_count().update(|count| *count += 1u64);
        }

        info.status = next.clone();
        self.agent_info(agent).set(&info);

        self.status_changed_event(agent, self.status_to_code(&next), info.credit_score);
    }

    fn status_to_code(&self, status: &AgentStatus) -> u64 {
        match status {
            AgentStatus::Active => STATUS_ACTIVE,
            AgentStatus::Paused => STATUS_PAUSED,
            AgentStatus::Suspended => STATUS_SUSPENDED,
            AgentStatus::Cancelled => STATUS_CANCELLED,
        }
    }

    fn min_biguint(&self, a: &BigUint, b: &BigUint) -> BigUint {
        if a <= b {
            a.clone()
        } else {
            b.clone()
        }
    }

    fn require_agent_exists(&self, agent: &ManagedAddress) {
        require!(!self.agent_info(agent).is_empty(), "Agent not enrolled");
    }

    fn only_operator(&self) {
        let caller = self.blockchain().get_caller();
        require!(caller == self.operator().get(), "Only operator");
    }

    fn only_owner(&self) {
        let caller = self.blockchain().get_caller();
        require!(caller == self.owner().get(), "Only owner");
    }

    #[event("registered")]
    fn registered_event(
        &self,
        #[indexed] agent: &ManagedAddress,
        #[indexed] fee_bps: u64,
        #[indexed] max_windows_per_epoch: u64,
        bond_added: &BigUint,
    );

    #[event("bondTopup")]
    fn bond_topped_up_event(&self, #[indexed] agent: &ManagedAddress, amount: &BigUint);

    #[event("epochBilled")]
    fn epoch_billed_event(
        &self,
        #[indexed] agent: &ManagedAddress,
        #[indexed] epoch: u64,
        #[indexed] windows: u64,
        due: &BigUint,
    );

    #[event("epochSettled")]
    fn epoch_settled_event(
        &self,
        #[indexed] agent: &ManagedAddress,
        #[indexed] epoch: u64,
        paid: &BigUint,
    );

    #[event("epochEnforced")]
    fn epoch_enforced_event(
        &self,
        #[indexed] agent: &ManagedAddress,
        #[indexed] epoch: u64,
        slashed: &BigUint,
    );

    #[event("statusChanged")]
    fn status_changed_event(
        &self,
        #[indexed] agent: &ManagedAddress,
        #[indexed] status_code: u64,
        credit_score: u64,
    );

    #[event("cancelled")]
    fn cancelled_event(&self, #[indexed] agent: &ManagedAddress, refunded_bond: &BigUint);

    #[event("ownerWithdrawn")]
    fn owner_withdrawn_event(&self, #[indexed] to: &ManagedAddress, amount: &BigUint);

    #[storage_mapper("owner")]
    fn owner(&self) -> SingleValueMapper<ManagedAddress>;

    #[storage_mapper("operator")]
    fn operator(&self) -> SingleValueMapper<ManagedAddress>;

    #[storage_mapper("windowReward")]
    fn window_reward(&self) -> SingleValueMapper<BigUint>;

    #[storage_mapper("setupFee")]
    fn setup_fee(&self) -> SingleValueMapper<BigUint>;

    #[storage_mapper("minBond")]
    fn min_bond(&self) -> SingleValueMapper<BigUint>;

    #[storage_mapper("graceEpochs")]
    fn grace_epochs(&self) -> SingleValueMapper<u64>;

    #[storage_mapper("maxBackbillEpochs")]
    fn max_backbill_epochs(&self) -> SingleValueMapper<u64>;

    #[storage_mapper("hardMaxWindowsPerEpoch")]
    fn hard_max_windows_per_epoch(&self) -> SingleValueMapper<u64>;

    #[storage_mapper("promoFreeSlots")]
    fn promo_free_slots(&self) -> SingleValueMapper<u64>;

    #[storage_mapper("promoUsed")]
    fn promo_used(&self) -> SingleValueMapper<u64>;

    #[storage_mapper("claimableOwner")]
    fn claimable_owner(&self) -> SingleValueMapper<BigUint>;

    #[storage_mapper("activeAgentCount")]
    fn active_agent_count(&self) -> SingleValueMapper<u64>;

    #[storage_mapper("agentInfo")]
    fn agent_info(&self, agent: &ManagedAddress) -> SingleValueMapper<AgentInfo<Self::Api>>;

    #[storage_mapper("bondBalance")]
    fn bond_balance(&self, agent: &ManagedAddress) -> SingleValueMapper<BigUint>;

    #[storage_mapper("outstandingTotal")]
    fn outstanding_total(&self, agent: &ManagedAddress) -> SingleValueMapper<BigUint>;

    #[storage_mapper("epochWindows")]
    fn epoch_windows(&self, agent: &ManagedAddress, epoch: u64) -> SingleValueMapper<u64>;

    #[storage_mapper("epochDue")]
    fn epoch_due(&self, agent: &ManagedAddress, epoch: u64) -> SingleValueMapper<BigUint>;

    #[storage_mapper("epochDeadline")]
    fn epoch_deadline(&self, agent: &ManagedAddress, epoch: u64) -> SingleValueMapper<u64>;

    #[storage_mapper("epochScoreApplied")]
    fn epoch_score_applied(&self, agent: &ManagedAddress, epoch: u64) -> SingleValueMapper<bool>;

    #[storage_mapper("epochState")]
    fn epoch_state(&self, agent: &ManagedAddress, epoch: u64) -> SingleValueMapper<EpochState>;
}

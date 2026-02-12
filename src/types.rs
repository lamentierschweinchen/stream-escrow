multiversx_sc::imports!();
multiversx_sc::derive_imports!();

#[type_abi]
#[derive(TopEncode, TopDecode, NestedEncode, NestedDecode, Clone, PartialEq, Eq)]
pub enum AgentStatus {
    Active,
    Paused,
    Suspended,
    Cancelled,
}

#[type_abi]
#[derive(TopEncode, TopDecode, NestedEncode, NestedDecode, Clone, PartialEq, Eq)]
pub enum EpochState {
    Unbilled,
    Billed,
    SettledOnTime,
    SettledLate,
    Slashed,
    Delinquent,
}

#[type_abi]
#[derive(TopEncode, TopDecode, NestedEncode, NestedDecode, Clone)]
pub struct AgentInfo<M: ManagedTypeApi> {
    pub fee_bps: u64,
    pub max_windows_per_epoch: u64,
    pub max_charge_per_epoch: BigUint<M>,
    pub credit_score: u64,
    pub status: AgentStatus,
    pub used_promo: bool,
    pub joined_epoch: u64,
    pub last_billed_epoch: u64,
    pub metadata: ManagedBuffer<M>,
}

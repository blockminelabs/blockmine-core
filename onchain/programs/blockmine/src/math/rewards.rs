pub const ERA_NAME_LEN: usize = 16;
pub const TOTAL_PROTOCOL_EMISSIONS: u64 = 20_000_000_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RewardEra {
    pub index: u8,
    pub name: [u8; ERA_NAME_LEN],
    pub reward: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RewardEraSpec {
    start_block: u64,
    end_block_exclusive: Option<u64>,
    reward: u64,
    name: [u8; ERA_NAME_LEN],
}

const DECIMALS: u64 = 1_000_000_000;

const fn era_name(name: &[u8]) -> [u8; ERA_NAME_LEN] {
    let mut out = [0u8; ERA_NAME_LEN];
    let mut i = 0;
    while i < name.len() && i < ERA_NAME_LEN {
        out[i] = name[i];
        i += 1;
    }
    out
}

const fn bloc(amount: u64) -> u64 {
    amount * DECIMALS
}

const fn deci_bloc(amount_tenths: u64) -> u64 {
    amount_tenths * (DECIMALS / 10)
}

const fn centi_bloc(amount_hundredths: u64) -> u64 {
    amount_hundredths * (DECIMALS / 100)
}

const SCARCITY_START_BLOCK: u64 = 16_000_000;
const SCARCITY_BASE_REWARD: u64 = centi_bloc(15);
const SCARCITY_FULL_REWARD_BLOCKS: u64 = 6_466_666;
const SCARCITY_FINAL_PARTIAL_REWARD: u64 = deci_bloc(1);

const REWARD_ERAS: [RewardEraSpec; 14] = [
    RewardEraSpec {
        start_block: 0,
        end_block_exclusive: Some(10_000),
        reward: bloc(21),
        name: era_name(b"Genesis"),
    },
    RewardEraSpec {
        start_block: 10_000,
        end_block_exclusive: Some(100_000),
        reward: bloc(12),
        name: era_name(b"Aurum"),
    },
    RewardEraSpec {
        start_block: 100_000,
        end_block_exclusive: Some(300_000),
        reward: bloc(7),
        name: era_name(b"Phoenix"),
    },
    RewardEraSpec {
        start_block: 300_000,
        end_block_exclusive: Some(600_000),
        reward: bloc(5),
        name: era_name(b"Horizon"),
    },
    RewardEraSpec {
        start_block: 600_000,
        end_block_exclusive: Some(1_000_000),
        reward: deci_bloc(38),
        name: era_name(b"Quasar"),
    },
    RewardEraSpec {
        start_block: 1_000_000,
        end_block_exclusive: Some(1_500_000),
        reward: bloc(3),
        name: era_name(b"Pulsar"),
    },
    RewardEraSpec {
        start_block: 1_500_000,
        end_block_exclusive: Some(2_100_000),
        reward: deci_bloc(23),
        name: era_name(b"Voidfall"),
    },
    RewardEraSpec {
        start_block: 2_100_000,
        end_block_exclusive: Some(3_000_000),
        reward: deci_bloc(18),
        name: era_name(b"Eclipse"),
    },
    RewardEraSpec {
        start_block: 3_000_000,
        end_block_exclusive: Some(4_200_000),
        reward: deci_bloc(14),
        name: era_name(b"Mythos"),
    },
    RewardEraSpec {
        start_block: 4_200_000,
        end_block_exclusive: Some(5_800_000),
        reward: deci_bloc(11),
        name: era_name(b"Paragon"),
    },
    RewardEraSpec {
        start_block: 5_800_000,
        end_block_exclusive: Some(7_500_000),
        reward: deci_bloc(9),
        name: era_name(b"Hyperion"),
    },
    RewardEraSpec {
        start_block: 7_500_000,
        end_block_exclusive: Some(9_500_000),
        reward: deci_bloc(7),
        name: era_name(b"Singularity"),
    },
    RewardEraSpec {
        start_block: 9_500_000,
        end_block_exclusive: Some(12_000_000),
        reward: deci_bloc(5),
        name: era_name(b"Eternal I"),
    },
    RewardEraSpec {
        start_block: 12_000_000,
        end_block_exclusive: Some(16_000_000),
        reward: deci_bloc(3),
        name: era_name(b"Eternal II"),
    },
];

pub fn reward_era_for_block(block_number: u64) -> RewardEra {
    if block_number >= SCARCITY_START_BLOCK {
        let scarcity_offset = block_number - SCARCITY_START_BLOCK;
        let scarcity_reward = if scarcity_offset < SCARCITY_FULL_REWARD_BLOCKS {
            SCARCITY_BASE_REWARD
        } else if scarcity_offset == SCARCITY_FULL_REWARD_BLOCKS {
            SCARCITY_FINAL_PARTIAL_REWARD
        } else {
            0
        };

        return RewardEra {
            index: 14,
            name: era_name(b"Scarcity"),
            reward: scarcity_reward,
        };
    }

    for (index, era) in REWARD_ERAS.iter().enumerate() {
        let in_range = match era.end_block_exclusive {
            Some(end) => block_number >= era.start_block && block_number < end,
            None => block_number >= era.start_block,
        };

        if in_range {
            return RewardEra {
                index: index as u8,
                name: era.name,
                reward: era.reward,
            };
        }
    }

    let era = REWARD_ERAS[REWARD_ERAS.len() - 1];
    RewardEra {
        index: (REWARD_ERAS.len() - 1) as u8,
        name: era.name,
        reward: era.reward,
    }
}

pub fn reward_for_block(block_number: u64) -> u64 {
    reward_era_for_block(block_number).reward
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trim_name(name: [u8; ERA_NAME_LEN]) -> String {
        let end = name.iter().position(|byte| *byte == 0).unwrap_or(name.len());
        String::from_utf8(name[..end].to_vec()).unwrap()
    }

    #[test]
    fn reward_schedule_starts_with_genesis() {
        let era = reward_era_for_block(0);
        assert_eq!(trim_name(era.name), "Genesis");
        assert_eq!(era.reward, 21_000_000_000);
    }

    #[test]
    fn reward_schedule_switches_on_boundaries() {
        assert_eq!(trim_name(reward_era_for_block(9_999).name), "Genesis");
        assert_eq!(trim_name(reward_era_for_block(10_000).name), "Aurum");
        assert_eq!(trim_name(reward_era_for_block(100_000).name), "Phoenix");
        assert_eq!(trim_name(reward_era_for_block(16_000_000).name), "Scarcity");
    }

    #[test]
    fn reward_schedule_values_match_table() {
        assert_eq!(reward_for_block(0), 21_000_000_000);
        assert_eq!(reward_for_block(10_000), 12_000_000_000);
        assert_eq!(reward_for_block(600_000), 3_800_000_000);
        assert_eq!(reward_for_block(2_100_000), 1_800_000_000);
        assert_eq!(reward_for_block(5_800_000), 900_000_000);
        assert_eq!(reward_for_block(16_000_000), 150_000_000);
        assert_eq!(reward_for_block(22_466_666), 100_000_000);
        assert_eq!(reward_for_block(22_466_667), 0);
    }

    #[test]
    fn cumulative_emissions_match_exact_20m_schedule() {
        let emitted = REWARD_ERAS
            .iter()
            .map(|era| {
                let end = era.end_block_exclusive.expect("bounded era");
                (end - era.start_block) as u128 * era.reward as u128
            })
            .sum::<u128>()
            + (SCARCITY_FULL_REWARD_BLOCKS as u128 * SCARCITY_BASE_REWARD as u128)
            + SCARCITY_FINAL_PARTIAL_REWARD as u128;

        assert_eq!(emitted, TOTAL_PROTOCOL_EMISSIONS as u128);
    }
}

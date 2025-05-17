use std::{cmp::Ordering, collections::HashSet};

use alloy::primitives::{Address, BlockNumber};

// https://github.com/tendermint/tendermint/pull/2785#discussion_r235038971
// 1.125
const PENALTY_FACTOR: u64 = 1125;
/// do the math with fixed here to avoid floats
const ONE_E3: u64 = 1000;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct AngstromValidator {
    pub peer_id:  Address,
    voting_power: u64,
    priority:     i64
}

impl AngstromValidator {
    pub fn new(name: Address, voting_power: u64) -> Self {
        AngstromValidator {
            peer_id:      name,
            voting_power: voting_power * ONE_E3,
            priority:     0
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct WeightedRoundRobin {
    validators:                HashSet<AngstromValidator>,
    new_joiner_penalty_factor: u64,
    block_number:              BlockNumber,
    last_proposer:             Option<Address>
}

impl WeightedRoundRobin {
    pub fn new(validators: Vec<AngstromValidator>, start_block: BlockNumber) -> Self {
        WeightedRoundRobin {
            validators:                HashSet::from_iter(validators),
            new_joiner_penalty_factor: PENALTY_FACTOR,
            block_number:              start_block,
            last_proposer:             None
        }
    }

    pub fn get_validator_state(&self) -> HashSet<AngstromValidator> {
        self.validators.clone()
    }

    fn proposer_selection(&mut self) -> Address {
        let total_voting_power: u64 = self.validators.iter().map(|v| v.voting_power).sum();

        //  apply all priorities.
        self.validators = self
            .validators
            .drain()
            .map(|mut validator| {
                validator.priority += validator.voting_power as i64;
                validator
            })
            .collect();

        // find the max
        let mut proposer = self
            .validators
            .iter()
            .max_by(Self::priority)
            .unwrap()
            .clone();
        proposer.priority -= total_voting_power as i64;
        let proposer_name = proposer.peer_id;

        self.validators.replace(proposer);

        proposer_name
    }

    fn priority(a: &&AngstromValidator, b: &&AngstromValidator) -> Ordering {
        let out = a.priority.partial_cmp(&b.priority);
        if out == Some(Ordering::Equal) {
            // TODO: not the best because it encourages mining lower peer ids
            // however we need a way for this to be uniform across nodes and
            // this is the easiest
            return a.peer_id.cmp(&b.peer_id);
        }
        out.unwrap()
    }

    fn center_priorities(&mut self) {
        let avg_priority =
            self.validators.iter().map(|v| v.priority).sum::<i64>() / self.validators.len() as i64;

        self.validators = self
            .validators
            .drain()
            .map(|mut validator| {
                validator.priority -= avg_priority;
                validator
            })
            .collect();
    }

    fn scale_priorities(&mut self) {
        let max_priority = self
            .validators
            .iter()
            .map(|v| v.priority)
            .fold(i64::MIN, i64::max);
        let min_priority = self
            .validators
            .iter()
            .map(|v| v.priority)
            .fold(i64::MAX, i64::min);

        let total_voting_power: u64 = self.validators.iter().map(|v| v.voting_power).sum();
        let diff = max_priority - min_priority;
        let threshold = 2 * total_voting_power as i64;

        if diff > threshold {
            let scale = (diff * ONE_E3 as i64) / threshold;

            self.validators = self
                .validators
                .drain()
                .map(|mut validator| {
                    let new_pri = validator.priority * ONE_E3 as i64;
                    validator.priority = new_pri / scale;
                    validator
                })
                .collect();
        }
    }

    pub fn choose_proposer(&mut self, block_number: BlockNumber) -> Option<Address> {
        // 1. this is not ideal, since on multi-block reorgs the same proposer will be
        //    chosen for the length of the reorg
        // 2. reverting the block number (self.block_number = block_number) is also not
        //    ideal, since nodes who were offline will not have seen the reorg, thus
        //    would not have executed the extra rounds after this if statement
        if block_number == self.block_number {
            if self.last_proposer.is_none() {
                self.last_proposer = Some(self.proposer_selection());
            }

            return self.last_proposer;
        }

        let rounds_to_catchup = (block_number - self.block_number) as usize;
        let mut leader = None;
        for _ in 0..rounds_to_catchup {
            self.center_priorities();
            self.scale_priorities();
            leader = Some(self.proposer_selection());
            self.last_proposer = leader
        }
        self.block_number = block_number;
        leader
    }

    #[allow(dead_code)]
    fn remove_validator(&mut self, peer_id: &Address) {
        let validator = AngstromValidator::new(*peer_id, 0);
        self.validators.remove(&validator);
    }

    #[allow(dead_code)]
    fn add_validator(&mut self, peer_id: Address, voting_power: u64) {
        let mut new_validator = AngstromValidator::new(peer_id, voting_power);
        let total_voting_power: u64 = self.validators.iter().map(|v| v.voting_power).sum();
        new_validator.priority -=
            ((self.new_joiner_penalty_factor * total_voting_power) / ONE_E3) as i64;
        self.validators.insert(new_validator);
    }
}

impl PartialEq for AngstromValidator {
    fn eq(&self, other: &Self) -> bool {
        self.peer_id == other.peer_id
    }
}

impl Eq for AngstromValidator {}

impl std::hash::Hash for AngstromValidator {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.peer_id.hash(state);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use alloy::primitives::BlockNumber;

    use super::*;

    #[test]
    fn test_validator_equality() {
        let peer_id = Address::random();
        let v1 = AngstromValidator::new(peer_id, 100);
        let v2 = AngstromValidator::new(peer_id, 200);
        let v3 = AngstromValidator::new(Address::random(), 100);

        assert_eq!(v1, v2, "Validators with same peer_id should be equal");
        assert_ne!(v1, v3, "Validators with different peer_id should not be equal");
    }

    #[test]
    fn test_validator_hash() {
        use std::{
            collections::hash_map::DefaultHasher,
            hash::{Hash, Hasher}
        };

        let peer_id = Address::random();
        let v1 = AngstromValidator::new(peer_id, 100);
        let v2 = AngstromValidator::new(peer_id, 200);

        let mut hasher1 = DefaultHasher::new();
        let mut hasher2 = DefaultHasher::new();
        v1.hash(&mut hasher1);
        v2.hash(&mut hasher2);

        assert_eq!(hasher1.finish(), hasher2.finish(), "Hash should only depend on peer_id");
    }

    #[test]
    fn test_add_remove_validator() {
        let (_, validators) = create_test_validators();
        let mut algo = WeightedRoundRobin::new(validators, BlockNumber::default());

        // Test adding new validator
        let new_peer = Address::random();
        let initial_count = algo.validators.len();
        algo.add_validator(new_peer, 150);
        assert_eq!(algo.validators.len(), initial_count + 1);

        // Verify penalty was applied
        let new_validator = algo
            .validators
            .iter()
            .find(|v| v.peer_id == new_peer)
            .unwrap();
        assert!(new_validator.priority < 0, "New validator should have negative priority");

        // Test removing validator
        algo.remove_validator(&new_peer);
        assert_eq!(algo.validators.len(), initial_count);
        assert!(algo.validators.iter().all(|v| v.peer_id != new_peer));
    }

    #[test]
    fn test_priority_comparison() {
        let peer1 = Address::random();
        let peer2 = Address::random();

        let v1 =
            AngstromValidator { peer_id: peer1, voting_power: 100 * ONE_E3, priority: 10 };

        let v2 =
            AngstromValidator { peer_id: peer2, voting_power: 100 * ONE_E3, priority: 10 };

        let v3 =
            AngstromValidator { peer_id: peer2, voting_power: 100 * ONE_E3, priority: 20 };

        // Test equal priorities
        assert_eq!(
            WeightedRoundRobin::priority(&&v1, &&v2),
            peer1.cmp(&peer2),
            "Equal priorities should compare peer_ids"
        );

        // Test different priorities
        assert_eq!(
            WeightedRoundRobin::priority(&&v1, &&v3),
            Ordering::Less,
            "Different priorities should compare normally"
        );
    }

    #[test]
    fn test_choose_proposer_same_block() {
        let (_, validators) = create_test_validators();
        let mut algo = WeightedRoundRobin::new(validators, 5);

        // First call should select a proposer
        let first_proposer = algo.choose_proposer(5).unwrap();

        // Subsequent calls for same block should return the same proposer
        for _ in 0..5 {
            assert_eq!(
                algo.choose_proposer(5).unwrap(),
                first_proposer,
                "Same block number should return same proposer"
            );
        }
    }

    #[test]
    fn test_voting_power_scaling() {
        let peer_id = Address::random();
        let validator = AngstromValidator::new(peer_id, 100);
        assert_eq!(validator.voting_power, 100 * ONE_E3, "Voting power should be scaled by ONE_E3");
    }

    fn create_test_validators() -> (HashMap<String, Address>, Vec<AngstromValidator>) {
        let peers = HashMap::from([
            ("Alice".to_string(), Address::random()),
            ("Bob".to_string(), Address::random()),
            ("Charlie".to_string(), Address::random())
        ]);
        let validators = vec![
            AngstromValidator::new(peers["Alice"], 100),
            AngstromValidator::new(peers["Bob"], 200),
            AngstromValidator::new(peers["Charlie"], 300),
        ];
        (peers, validators)
    }

    #[test]
    fn test_priority_calculation() {
        let (_, validators) = create_test_validators();
        let mut algo = WeightedRoundRobin::new(validators, BlockNumber::default());

        // Get initial priorities
        let initial_priorities: Vec<i64> = algo.validators.iter().map(|v| v.priority).collect();
        assert!(initial_priorities.iter().all(|&p| p == 0), "Initial priorities should be 0");

        // Record initial voting powers
        let initial_powers: Vec<u64> = algo.validators.iter().map(|v| v.voting_power).collect();

        // Test single round of priority updates
        algo.proposer_selection();

        // After proposer selection:
        // 1. All validators should have their priority increased by their voting power
        // 2. The selected validator (highest priority) should then have total_power
        //    subtracted
        let total_power: u64 = initial_powers.iter().sum();

        for validator in algo.validators.iter() {
            if validator.priority < 0 {
                // This was the selected validator
                let expected_priority = validator.voting_power as i64 - total_power as i64;
                assert_eq!(
                    validator.priority, expected_priority,
                    "Selected validator should have priority = (own_power - total_power)"
                );
            } else {
                // Non-selected validators just got their voting power added
                assert_eq!(
                    validator.priority, validator.voting_power as i64,
                    "Non-selected validator should have priority = own_power"
                );
            }
        }
    }

    #[test]
    fn test_priority_centering() {
        let (_, validators) = create_test_validators();
        let mut algo = WeightedRoundRobin::new(validators, BlockNumber::default());

        // Set priorities to unscaled voting powers to test centering
        let max_power = 300; // Matches max voting power in create_test_validators
        algo.validators = algo
            .validators
            .drain()
            .map(|mut v| {
                // Use unscaled value to avoid massive numbers
                v.priority = (v.voting_power / ONE_E3) as i64;
                v
            })
            .collect();

        // Center priorities
        algo.center_priorities();

        // After centering:
        // 1. Sum should be close to zero (within rounding error)
        let sum_priorities: i64 = algo.validators.iter().map(|v| v.priority).sum();

        assert!(
            sum_priorities.abs() <= algo.validators.len() as i64,
            "Sum of centered priorities ({}) should be close to zero",
            sum_priorities
        );

        // 2. Each priority should be within reasonable bounds
        for validator in algo.validators.iter() {
            assert!(
                validator.priority.abs() <= max_power,
                "Individual priority ({}) should be within reasonable bounds",
                validator.priority
            );
        }

        // 3. Verify relative differences are maintained
        let priorities: Vec<i64> = algo.validators.iter().map(|v| v.priority).collect();
        let max_priority = priorities.iter().max().unwrap();
        let min_priority = priorities.iter().min().unwrap();
        assert!((max_priority - min_priority) <= max_power, "Priority spread should be reasonable");
    }

    #[test]
    fn test_priority_scaling() {
        let (_, validators) = create_test_validators();
        let mut algo = WeightedRoundRobin::new(validators, BlockNumber::default());

        // Set extreme priorities to trigger scaling
        let total_power: u64 = algo.validators.iter().map(|v| v.voting_power).sum();
        algo.validators = algo
            .validators
            .drain()
            .enumerate()
            .map(|(i, mut v)| {
                v.priority = (i as i64) * (total_power as i64) * 3; // Create large differences
                v
            })
            .collect();

        // Scale priorities
        algo.scale_priorities();

        // Verify scaling reduced the difference
        let max_priority = algo.validators.iter().map(|v| v.priority).max().unwrap();
        let min_priority = algo.validators.iter().map(|v| v.priority).min().unwrap();

        assert!(
            (max_priority - min_priority) <= 2 * (total_power as i64),
            "Priority difference should be less than twice the total voting power"
        );
    }

    #[test]
    fn test_proposer_selection_determinism() {
        let (_, validators) = create_test_validators();
        let mut algo1 = WeightedRoundRobin::new(validators.clone(), BlockNumber::default());
        let mut algo2 = WeightedRoundRobin::new(validators, BlockNumber::default());

        // Run multiple rounds and verify both instances select the same proposers
        for i in 1..=10 {
            let proposer1 = algo1.choose_proposer(i);
            let proposer2 = algo2.choose_proposer(i);
            assert_eq!(proposer1, proposer2, "Proposer selection should be deterministic");
        }
    }

    #[test]
    fn test_round_robin_simulation() {
        let peers = HashMap::from([
            ("Alice".to_string(), Address::random()),
            ("Bob".to_string(), Address::random()),
            ("Charlie".to_string(), Address::random())
        ]);
        let validators = vec![
            AngstromValidator::new(peers["Alice"], 100),
            AngstromValidator::new(peers["Bob"], 200),
            AngstromValidator::new(peers["Charlie"], 300),
        ];
        let mut algo = WeightedRoundRobin::new(validators, BlockNumber::default());

        fn simulate_rounds(
            algo: &mut WeightedRoundRobin,
            rounds: usize
        ) -> HashMap<Address, usize> {
            let mut stats = HashMap::new();
            for i in 1..=rounds {
                let proposer = algo.choose_proposer(BlockNumber::from(i as u64)).unwrap();
                *stats.entry(proposer).or_insert(0) += 1;
            }
            stats
        }

        let rounds = 1000;
        let stats = simulate_rounds(&mut algo, rounds);

        assert_eq!(stats.len(), 3);

        let total_selections: usize = stats.values().sum();
        assert_eq!(total_selections, rounds);

        let alice_ratio = *stats.get(&peers["Alice"]).unwrap() as f64 / rounds as f64;
        let bob_ratio = *stats.get(&peers["Bob"]).unwrap() as f64 / rounds as f64;
        let charlie_ratio = *stats.get(&peers["Charlie"]).unwrap() as f64 / rounds as f64;

        assert!((alice_ratio - 0.167).abs() < 0.05);
        assert!((bob_ratio - 0.333).abs() < 0.05);
        assert!((charlie_ratio - 0.5).abs() < 0.05);
    }
}

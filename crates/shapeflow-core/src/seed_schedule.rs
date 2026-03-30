use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

pub const TRAJECTORY_OFFSET: u64 = 1_000_000;
pub const TEXT_GRAMMAR_OFFSET: u64 = 2_000_000;
pub const LEXICAL_NOISE_OFFSET: u64 = 3_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneSeedSchedule {
    pub scene_layout: u64,
    pub trajectory: u64,
    pub text_grammar: u64,
    pub lexical_noise: u64,
}

impl SceneSeedSchedule {
    pub fn derive(master_seed: u64, scene_index: u64) -> Self {
        let scene_layout = checked_seed_add(
            master_seed,
            scene_index,
            "scene_layout(master_seed + scene_index)",
        );
        Self {
            scene_layout,
            trajectory: checked_seed_add(
                scene_layout,
                TRAJECTORY_OFFSET,
                "trajectory(scene_layout + TRAJECTORY_OFFSET)",
            ),
            text_grammar: checked_seed_add(
                scene_layout,
                TEXT_GRAMMAR_OFFSET,
                "text_grammar(scene_layout + TEXT_GRAMMAR_OFFSET)",
            ),
            lexical_noise: checked_seed_add(
                scene_layout,
                LEXICAL_NOISE_OFFSET,
                "lexical_noise(scene_layout + LEXICAL_NOISE_OFFSET)",
            ),
        }
    }

    pub fn scene_layout_rng(&self) -> ChaCha8Rng {
        ChaCha8Rng::seed_from_u64(self.scene_layout)
    }

    pub fn trajectory_rng(&self) -> ChaCha8Rng {
        ChaCha8Rng::seed_from_u64(self.trajectory)
    }

    pub fn text_grammar_rng(&self) -> ChaCha8Rng {
        ChaCha8Rng::seed_from_u64(self.text_grammar)
    }

    pub fn lexical_noise_rng(&self) -> ChaCha8Rng {
        ChaCha8Rng::seed_from_u64(self.lexical_noise)
    }
}

fn checked_seed_add(base: u64, offset: u64, context: &str) -> u64 {
    base.checked_add(offset).unwrap_or_else(|| {
        panic!("seed derivation overflow in {context}: base={base}, offset={offset}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::RngCore;

    #[test]
    fn derivation_is_deterministic() {
        let a = SceneSeedSchedule::derive(42, 7);
        let b = SceneSeedSchedule::derive(42, 7);
        assert_eq!(a, b);
    }

    #[test]
    fn streams_are_separated_by_offset() {
        let schedule = SceneSeedSchedule::derive(42, 7);
        assert_ne!(schedule.scene_layout, schedule.trajectory);
        assert_ne!(schedule.scene_layout, schedule.text_grammar);
        assert_ne!(schedule.scene_layout, schedule.lexical_noise);
        assert_ne!(schedule.trajectory, schedule.text_grammar);
        assert_ne!(schedule.trajectory, schedule.lexical_noise);
        assert_ne!(schedule.text_grammar, schedule.lexical_noise);
    }

    #[test]
    fn streams_are_separated_without_overflow() {
        let schedule = SceneSeedSchedule::derive(1_000, 2_000);
        assert_eq!(schedule.scene_layout, 3_000);
        assert_eq!(schedule.trajectory, 1_003_000);
        assert_eq!(schedule.text_grammar, 2_003_000);
        assert_eq!(schedule.lexical_noise, 3_003_000);
    }

    #[test]
    #[should_panic(expected = "seed derivation overflow")]
    fn derive_panics_when_scene_layout_overflows() {
        let _ = SceneSeedSchedule::derive(u64::MAX, 1);
    }

    #[test]
    #[should_panic(expected = "seed derivation overflow")]
    fn derive_panics_when_offset_stream_overflows() {
        let _ = SceneSeedSchedule::derive(u64::MAX, 0);
    }

    #[test]
    fn rng_streams_do_not_match() {
        let schedule = SceneSeedSchedule::derive(9, 12);

        let mut layout_rng = schedule.scene_layout_rng();
        let mut trajectory_rng = schedule.trajectory_rng();
        let mut grammar_rng = schedule.text_grammar_rng();

        let layout_value = layout_rng.next_u64();
        let trajectory_value = trajectory_rng.next_u64();
        let grammar_value = grammar_rng.next_u64();

        assert_ne!(layout_value, trajectory_value);
        assert_ne!(layout_value, grammar_value);
        assert_ne!(trajectory_value, grammar_value);
    }
}

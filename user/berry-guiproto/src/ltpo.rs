//! Software LTPO cadence policy for BerryOS display services.
//!
//! QEMU virtio-gpu exposes no physical panel refresh-rate control. This pure
//! policy therefore models the *software discipline* an LTPO controller needs:
//! interactive submissions boost to the display cap, an inactive pipeline
//! steps down 60→30→10→1Hz, sticky idle and AOD stay at 1Hz, and OFF stops
//! cadence. The caller remains the display-service authority; UI ballots and
//! SUBMIT `rate_hint` are inputs, not direct frequency commands.

/// QEMU display cap for this topology (1080×1920@60).
pub const LTPO_HZ_CAP: u16 = 60;
/// Longest software cadence used while a display remains logically on.
pub const LTPO_HZ_FLOOR: u16 = 1;
/// Inactivity age below which the pipeline remains at cap.
pub const LTPO_BOOST_NS: u64 = 250_000_000;
/// First step-down age (60→30Hz).
pub const LTPO_STEP_30_NS: u64 = 1_000_000_000;
/// Second step-down age (30→10Hz).
pub const LTPO_STEP_10_NS: u64 = 3_000_000_000;

/// Resolve the current cadence. `idle_ns` is time since the last real SUBMIT.
///
/// The result is one of `{0,1,10,30,60}`. Zero means the display service must
/// not schedule soft vblank at all (OFF). This function never mutates state,
/// so host tests can pin every policy edge without a panel or QEMU run.
pub fn software_ltpo_hz(
    min_hz: u16,
    max_hz: u16,
    preferred_hz: u16,
    sticky: u8,
    power_mode: u8,
    idle_ns: u64,
) -> u8 {
    if power_mode == crate::POWER_OFF {
        return 0;
    }
    if power_mode == crate::POWER_AOD {
        return 1;
    }

    let ballot_all_zero = min_hz == 0 && max_hz == 0 && preferred_hz == 0;
    if sticky != 0 || ballot_all_zero {
        return 1;
    }

    // Dim may run a maintenance cadence but must not override AOD/OFF.
    let mut target = if preferred_hz > 0 {
        preferred_hz
    } else {
        max_hz
    };
    if power_mode == crate::POWER_DIM {
        target = target.min(30);
    }

    // Inactivity ladder only lowers cadence; any fresh SUBMIT is observed by
    // the caller as idle_ns=0 and therefore stays at the interactive target.
    if idle_ns >= LTPO_STEP_10_NS {
        target = target.min(10);
    } else if idle_ns >= LTPO_STEP_30_NS {
        target = target.min(30);
    } else if idle_ns >= LTPO_BOOST_NS {
        target = target.min(30);
    }

    // Ballot interval intersects the emulator's hard 60Hz display cap.
    target = target.min(LTPO_HZ_CAP).min(max_hz.max(1));
    if min_hz > 0 {
        target = target.max(min_hz.min(LTPO_HZ_CAP));
    }
    let snapped = match target {
        0..=1 => 1,
        2..=10 => 10,
        11..=30 => 30,
        _ => LTPO_HZ_CAP as u8,
    };
    // DIM is a display-service power state, not a UI ballot: it caps even a
    // mandatory 60Hz interactive floor at the maintenance cadence.
    if power_mode == crate::POWER_DIM {
        return snapped.min(30);
    }
    snapped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interactive_ballot_is_capped_at_sixty() {
        assert_eq!(
            software_ltpo_hz(60, 120, 120, 0, crate::POWER_NORMAL, 0),
            60
        );
    }

    #[test]
    fn animation_and_normal_ballots_stay_at_sixty_when_fresh() {
        assert_eq!(
            software_ltpo_hz(30, 120, 60, 0, crate::POWER_NORMAL, 0),
            60
        );
        assert_eq!(software_ltpo_hz(0, 60, 60, 0, crate::POWER_NORMAL, 0), 60);
    }

    #[test]
    fn inactivity_steps_down_and_submissions_do_not() {
        let vote = (0u16, 60u16, 60u16, 0u8);
        assert_eq!(
            software_ltpo_hz(vote.0, vote.1, vote.2, vote.3, crate::POWER_NORMAL, 249_999_999),
            60
        );
        assert_eq!(
            software_ltpo_hz(vote.0, vote.1, vote.2, vote.3, crate::POWER_NORMAL, 250_000_000),
            30
        );
        assert_eq!(
            software_ltpo_hz(vote.0, vote.1, vote.2, vote.3, crate::POWER_NORMAL, 1_000_000_000),
            30
        );
        assert_eq!(
            software_ltpo_hz(vote.0, vote.1, vote.2, vote.3, crate::POWER_NORMAL, 3_000_000_000),
            10
        );
    }

    #[test]
    fn sticky_idle_and_power_modes_have_explicit_cadence() {
        assert_eq!(
            software_ltpo_hz(0, 0, 0, 1, crate::POWER_NORMAL, 0),
            1,
            "sticky idle stays logically on at maintenance cadence"
        );
        assert_eq!(software_ltpo_hz(60, 120, 120, 0, crate::POWER_DIM, 0), 30);
        assert_eq!(software_ltpo_hz(60, 120, 120, 0, crate::POWER_AOD, 0), 1);
        assert_eq!(software_ltpo_hz(60, 120, 120, 0, crate::POWER_OFF, 0), 0);
    }

    #[test]
    fn ballot_minimum_cannot_be_silently_dropped() {
        assert_eq!(
            software_ltpo_hz(30, 60, 10, 0, crate::POWER_NORMAL, 3_000_000_000),
            30
        );
    }
}

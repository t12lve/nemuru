//! Courbes de baisse du volume. Fonctions pures, testées sans Windows.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FadeCurve {
    Linear,
    Logarithmic,
    Sigmoid,
    LateFade,
}

impl FadeCurve {
    pub const ALL: [FadeCurve; 4] = [
        FadeCurve::LateFade,
        FadeCurve::Logarithmic,
        FadeCurve::Linear,
        FadeCurve::Sigmoid,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FadeCurve::Linear => "Linéaire",
            FadeCurve::Logarithmic => "À l'oreille",
            FadeCurve::Sigmoid => "En douceur",
            FadeCurve::LateFade => "Sur la fin",
        }
    }

    pub fn detail(self) -> &'static str {
        match self {
            FadeCurve::Linear => "Le niveau descend d'un pas égal chaque minute.",
            FadeCurve::Logarithmic => "La baisse semble régulière à l'oreille.",
            FadeCurve::Sigmoid => "La baisse commence et finit lentement.",
            FadeCurve::LateFade => "Le volume du moment tient, puis descend à la fin.",
        }
    }
}

/// Part du temps total occupée par la descente, pour la courbe « sur la fin ».
pub fn late_fraction(total_secs: f32, late_minutes: u64) -> f32 {
    if total_secs <= 1.0 {
        return 1.0;
    }
    let late = (late_minutes as f32) * 60.0;
    (late.clamp(1.0, total_secs)) / total_secs
}

/// Facteur appliqué au volume de départ. `progress` va de 0 (début) à 1 (fin).
/// `late_fraction` n'est lu que pour [`FadeCurve::LateFade`].
pub fn volume_factor(progress: f32, curve: FadeCurve, late_fraction: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    if progress >= 1.0 {
        return 0.0;
    }
    let factor = match curve {
        FadeCurve::Linear => 1.0 - progress,
        // Descente linéaire en décibels, de 0 dB à −60 dB.
        // C'est ce qui sonne régulier : l'amplitude, elle, chute plus vite au début.
        FadeCurve::Logarithmic => 10f32.powf(-60.0 * progress / 20.0),
        FadeCurve::Sigmoid => {
            let rem = 1.0 - progress;
            rem * rem * (3.0 - 2.0 * rem)
        }
        FadeCurve::LateFade => {
            let window = late_fraction.clamp(0.02, 1.0);
            let start = 1.0 - window;
            if progress <= start {
                1.0
            } else {
                let rem = (1.0 - progress) / window;
                rem.clamp(0.0, 1.0).powi(2)
            }
        }
    };
    factor.clamp(0.0, 1.0)
}

/// Si le volume observé s'éloigne de la dernière écriture, ce niveau devient
/// la nouvelle base. Le facteur de la courbe reste un pourcentage de cette base.
pub fn adopted_anchor(anchor: f32, last_written: f32, observed: f32, applied_factor: f32) -> f32 {
    let anchor = anchor.clamp(0.0, 1.0);
    let observed = observed.clamp(0.0, 1.0);
    if (observed - last_written).abs() <= 0.015 {
        return anchor;
    }
    if applied_factor > 0.05 {
        (observed / applied_factor).clamp(0.0, 1.0)
    } else {
        observed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints_are_full_then_silent() {
        for curve in FadeCurve::ALL {
            let start = volume_factor(0.0, curve, 0.25);
            let end = volume_factor(1.0, curve, 0.25);
            assert!((start - 1.0).abs() < 1e-4, "{curve:?} start {start}");
            assert!(end < 1e-4, "{curve:?} end {end}");
        }
    }

    #[test]
    fn curves_do_not_rise() {
        for curve in FadeCurve::ALL {
            let mut prev = volume_factor(0.0, curve, 0.25);
            for step in 1..=40 {
                let value = volume_factor(step as f32 / 40.0, curve, 0.25);
                assert!(
                    value <= prev + 1e-4,
                    "{curve:?} remonte à {step}: {value} > {prev}"
                );
                prev = value;
            }
        }
    }

    #[test]
    fn ear_curve_is_mid_quiet() {
        let mid = volume_factor(0.5, FadeCurve::Logarithmic, 0.25);
        // −30 dB ≈ 0,0316
        assert!((mid - 0.031622776).abs() < 1e-4, "{mid}");
    }

    #[test]
    fn late_curve_holds_then_drops() {
        let held = volume_factor(0.5, FadeCurve::LateFade, 0.25);
        assert!((held - 1.0).abs() < 1e-4);
        let dropped = volume_factor(0.9, FadeCurve::LateFade, 0.25);
        assert!(dropped < 0.5, "{dropped}");
    }

    #[test]
    fn late_fraction_respects_the_minutes() {
        let fraction = late_fraction(45.0 * 60.0, 10);
        assert!((fraction - (10.0 / 45.0)).abs() < 1e-4);
        assert_eq!(late_fraction(30.0, 10), 1.0);
    }

    #[test]
    fn a_small_readback_keeps_the_anchor() {
        let anchor = adopted_anchor(0.40, 0.40, 0.405, 1.0);
        assert!((anchor - 0.40).abs() < 1e-4);
    }

    #[test]
    fn a_slider_move_becomes_the_new_base() {
        let anchor = adopted_anchor(0.80, 0.80, 0.40, 1.0);
        assert!((anchor - 0.40).abs() < 1e-4);
    }

    #[test]
    fn a_move_during_the_fade_keeps_the_percentage() {
        let anchor = adopted_anchor(0.80, 0.40, 0.20, 0.50);
        assert!((anchor - 0.40).abs() < 1e-3);
    }

    #[test]
    fn a_move_at_the_floor_is_taken_as_is() {
        let anchor = adopted_anchor(0.80, 0.02, 0.30, 0.02);
        assert!((anchor - 0.30).abs() < 1e-4);
    }
}

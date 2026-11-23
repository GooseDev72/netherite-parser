//! The speed floor and the line it is measured against.
//!
//! A node is only as fast as the link under it. Asking for 15 Mbit/s on a
//! 15 Mbit/s line would reject every node and produce an empty subscription that
//! still looks like a successful run, so the line is measured first and the floor
//! adapts to it. On a fast line the floor stays at the configured 15 Mbit/s.

use crate::settings::Settings;

/// Measure this machine's own download speed, without any proxy.
pub async fn baseline(settings: &Settings) -> Option<f64> {
    v2keylib::tunnel::baseline_speed(
        &settings.speed_urls,
        settings.speed_bytes,
        settings.speed_timeout,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fast_line_keeps_the_brief_floor() {
        let settings = Settings::default();
        assert_eq!(settings.effective_threshold(Some(46.3)), 15.0);
    }

    #[test]
    fn a_line_at_the_floor_stays_there() {
        let settings = Settings::default();
        assert_eq!(settings.effective_threshold(Some(15.0)), 15.0);
    }

    #[test]
    fn a_slow_line_lowers_it() {
        let settings = Settings::default();
        assert_eq!(settings.effective_threshold(Some(10.0)), 7.5);
    }

    #[test]
    fn a_failed_baseline_keeps_the_floor() {
        let settings = Settings::default();
        assert_eq!(settings.effective_threshold(None), 15.0);
    }
}

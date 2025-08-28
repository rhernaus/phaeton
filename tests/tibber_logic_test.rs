#[cfg(feature = "tibber")]
mod tibber_logic {
    use phaeton::tibber::{PriceLevel, TibberClient};

    #[test]
    fn price_level_mapping_roundtrip() {
        assert_eq!(PriceLevel::from_label("VERY_CHEAP"), PriceLevel::VeryCheap);
        assert_eq!(PriceLevel::from_label("cheap"), PriceLevel::Cheap);
        assert_eq!(PriceLevel::from_label("normal"), PriceLevel::Normal);
        assert_eq!(PriceLevel::from_label("expensive"), PriceLevel::Expensive);
        assert_eq!(
            PriceLevel::from_label("very_expensive"),
            PriceLevel::VeryExpensive
        );
        assert_eq!(PriceLevel::VeryCheap.as_str(), "VERY_CHEAP");
    }

    #[tokio::test]
    async fn decide_should_charge_level_strategy_flags() {
        let client = TibberClient::new("token".to_string(), None);
        // Manually populate cache via private fields is not possible; we only test behavior that
        // does not require network: decision based on provided price_level and config flags.
        let cfg = phaeton::config::TibberConfig {
            access_token: "token".into(),
            home_id: String::new(),
            charge_on_cheap: true,
            charge_on_very_cheap: false,
            strategy: "level".into(),
            max_price_total: 0.0,
            cheap_percentile: 0.3,
        };
        // Without current_total populated, decide_should_charge should still consider level
        assert!(client.decide_should_charge(&cfg, Some(PriceLevel::Cheap)));
        assert!(!client.decide_should_charge(&cfg, Some(PriceLevel::VeryCheap)));
    }
}

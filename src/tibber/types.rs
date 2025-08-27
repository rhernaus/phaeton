/// Tibber price level mapping (only when feature is enabled)
#[cfg(feature = "tibber")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceLevel {
    VeryCheap,
    Cheap,
    Normal,
    Expensive,
    VeryExpensive,
}

#[cfg(feature = "tibber")]
impl PriceLevel {
    pub fn from_label(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "VERY_CHEAP" => Self::VeryCheap,
            "CHEAP" => Self::Cheap,
            "EXPENSIVE" => Self::Expensive,
            "VERY_EXPENSIVE" => Self::VeryExpensive,
            _ => Self::Normal,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::VeryCheap => "VERY_CHEAP",
            Self::Cheap => "CHEAP",
            Self::Normal => "NORMAL",
            Self::Expensive => "EXPENSIVE",
            Self::VeryExpensive => "VERY_EXPENSIVE",
        }
    }
}

#[cfg(feature = "tibber")]
#[derive(Debug, Clone)]
pub struct PricePoint {
    pub starts_at: String,
    pub total: f64,
    pub level: PriceLevel,
}

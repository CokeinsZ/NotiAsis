/// Error genérico para las fronteras donde se mezclan errores de varias
/// capas (pasos del pipeline, arranque). Dentro de cada capa se usan
/// errores tipados con `thiserror`.
pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

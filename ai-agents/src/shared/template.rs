use std::collections::{BTreeMap, BTreeSet};

/// Error al renderizar una plantilla.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TemplateError {
    #[error("missing values for placeholders: {0:?}")]
    MissingValues(Vec<String>),
    #[error("values without placeholder in the template: {0:?}")]
    UnknownValues(Vec<String>),
    #[error("unclosed placeholder starting at byte {0}")]
    Unclosed(usize),
}

/// Plantilla de texto con marcadores `{{nombre}}`.
///
/// El render es de una sola pasada: si un valor trae `{{...}}` (por
/// ejemplo, un mensaje de un cliente) se copia tal cual y no se vuelve a
/// expandir. Faltar un valor o sobrar uno es un error, para que un typo en
/// un prompt no pase desapercibido.
#[derive(Debug, Clone)]
pub struct PromptTemplate {
    source: String,
    placeholders: BTreeSet<String>,
}

/// Valores con los que se renderiza una plantilla.
#[derive(Debug, Clone, Default)]
pub struct TemplateValues(BTreeMap<String, String>);

impl TemplateValues {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, key: &str, value: impl Into<String>) -> Self {
        self.0.insert(key.to_string(), value.into());
        self
    }
}

impl PromptTemplate {
    pub fn parse(source: impl Into<String>) -> Result<Self, TemplateError> {
        let source = source.into();
        let mut placeholders = BTreeSet::new();
        for segment in segments(&source)? {
            if let Segment::Placeholder(name) = segment {
                placeholders.insert(name.to_string());
            }
        }
        Ok(Self { source, placeholders })
    }

    pub fn placeholders(&self) -> &BTreeSet<String> {
        &self.placeholders
    }

    pub fn render(&self, values: &TemplateValues) -> Result<String, TemplateError> {
        let missing: Vec<String> =
            self.placeholders.iter().filter(|name| !values.0.contains_key(*name)).cloned().collect();
        if !missing.is_empty() {
            return Err(TemplateError::MissingValues(missing));
        }
        let unknown: Vec<String> = values.0.keys().filter(|name| !self.placeholders.contains(*name)).cloned().collect();
        if !unknown.is_empty() {
            return Err(TemplateError::UnknownValues(unknown));
        }

        let mut output = String::with_capacity(self.source.len());
        for segment in segments(&self.source)? {
            match segment {
                Segment::Text(text) => output.push_str(text),
                Segment::Placeholder(name) => output.push_str(&values.0[name]),
            }
        }
        Ok(output)
    }
}

enum Segment<'a> {
    Text(&'a str),
    Placeholder(&'a str),
}

fn segments(source: &str) -> Result<Vec<Segment<'_>>, TemplateError> {
    let mut result = Vec::new();
    let mut rest = source;
    let mut offset = 0;
    while let Some(start) = rest.find("{{") {
        result.push(Segment::Text(&rest[..start]));
        let after_open = &rest[start + 2..];
        let end = after_open.find("}}").ok_or(TemplateError::Unclosed(offset + start))?;
        result.push(Segment::Placeholder(after_open[..end].trim()));
        let consumed = start + 2 + end + 2;
        offset += consumed;
        rest = &rest[consumed..];
    }
    result.push(Segment::Text(rest));
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_placeholders_with_or_without_spaces() {
        let template = PromptTemplate::parse("Hola {{ nombre }}, eres {{rol}}.").unwrap();
        let text = template.render(&TemplateValues::new().with("nombre", "Carmen").with("rol", "clienta")).unwrap();
        assert_eq!(text, "Hola Carmen, eres clienta.");
    }

    #[test]
    fn values_are_not_expanded_again() {
        let template = PromptTemplate::parse("Cliente: {{mensaje}}").unwrap();
        let text = template.render(&TemplateValues::new().with("mensaje", "{{mensaje}} {{x}}")).unwrap();
        assert_eq!(text, "Cliente: {{mensaje}} {{x}}");
    }

    #[test]
    fn missing_and_unknown_values_are_errors() {
        let template = PromptTemplate::parse("{{a}} {{b}}").unwrap();
        assert_eq!(
            template.render(&TemplateValues::new().with("a", "1")),
            Err(TemplateError::MissingValues(vec!["b".into()]))
        );
        assert_eq!(
            template.render(&TemplateValues::new().with("a", "1").with("b", "2").with("c", "3")),
            Err(TemplateError::UnknownValues(vec!["c".into()]))
        );
    }

    #[test]
    fn unclosed_placeholder_is_rejected() {
        assert_eq!(PromptTemplate::parse("hola {{nombre").unwrap_err(), TemplateError::Unclosed(5));
    }
}

//! Carga opcional del archivo `.env` (desarrollo local; en Docker las
//! variables llegan por docker-compose y no hay archivo).

/// Carga `.env` si existe. Si tiene un error de formato (p. ej. una comilla
/// doble suelta dentro de un valor sin comillas), dotenvy deja de leer ahí:
/// las variables siguientes no se cargan. Se avisa con el nombre de la
/// variable y nunca con la línea, porque la línea trae el secreto.
pub fn load_dotenv() {
    match dotenvy::dotenv() {
        Ok(_) => {}
        Err(error) if error.not_found() => {}
        Err(dotenvy::Error::LineParse(line, _)) => eprintln!(
            "aviso: .env tiene un error de formato en {}; esa variable y las siguientes no se cargaron \
             (pon el valor entre comillas simples)",
            variable_name(&line)
        ),
        Err(_) => eprintln!("aviso: no se pudo leer el archivo .env"),
    }
}

fn variable_name(line: &str) -> &str {
    line.split_once('=').map(|(name, _)| name.trim()).filter(|name| !name.is_empty()).unwrap_or("una línea")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_variable_without_its_value() {
        assert_eq!(variable_name("BACKEND_API_KEY=abc\"def\nOTRA=1"), "BACKEND_API_KEY");
        assert_eq!(variable_name("sin igual"), "una línea");
    }
}

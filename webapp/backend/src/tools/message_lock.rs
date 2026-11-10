//! Bloqueo por mensaje (wamid) dentro de una transacción.
//!
//! Meta acepta el envío de una plantilla y avisa por webhook, segundos
//! después, si falló. Ese aviso puede llegar antes o al mismo tiempo que
//! el bot registra el mensaje y marca la notificación de la guía. Las tres
//! operaciones toman este bloqueo para que la que llega segunda vea lo que
//! hizo la primera:
//!
//! - registrar el mensaje saliente -> nace 'failed' si ya hay error;
//! - marcar la notificación de la guía -> no avanza si ya hay error;
//! - registrar el error -> deshace la notificación si ya estaba marcada.

use sqlx::{Postgres, Transaction};

/// Errores de Meta que no se arreglan reenviando: 131026 (el número no
/// tiene WhatsApp, o no puede recibir el mensaje). El mensaje queda en
/// 'failed' con su error, pero la notificación no se deshace: se cuenta
/// como hecha para no reenviar la misma plantilla en cada corrida.
pub const PERMANENT_ERROR_CODES: [i32; 1] = [131026];

pub fn is_permanent_error(code: Option<i32>) -> bool {
    code.is_some_and(|code| PERMANENT_ERROR_CODES.contains(&code))
}

pub async fn lock_message(tx: &mut Transaction<'_, Postgres>, meta_message_id: &str) -> Result<(), String> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(meta_message_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Si Meta ya reportó un error del mensaje que obliga a reintentar la
/// notificación (cualquiera menos los permanentes).
pub async fn retryable_failure(tx: &mut Transaction<'_, Postgres>, meta_message_id: &str) -> Result<bool, String> {
    let (failed,): (bool,) = sqlx::query_as(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM messages_errors
            WHERE meta_message_id = $1
              AND (error_code IS NULL OR NOT (error_code = ANY($2)))
        )
        "#,
    )
    .bind(meta_message_id)
    .bind(&PERMANENT_ERROR_CODES[..])
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| e.to_string())?;
    Ok(failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_undeliverable_numbers_are_permanent() {
        assert!(is_permanent_error(Some(131026)));
        assert!(!is_permanent_error(Some(131048))); // límite: se reintenta
        assert!(!is_permanent_error(None));
    }
}

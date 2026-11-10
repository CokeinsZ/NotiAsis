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

pub async fn lock_message(tx: &mut Transaction<'_, Postgres>, meta_message_id: &str) -> Result<(), String> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(meta_message_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Si Meta ya reportó un error del mensaje (tabla messages_errors).
pub async fn message_failed(tx: &mut Transaction<'_, Postgres>, meta_message_id: &str) -> Result<bool, String> {
    let (failed,): (bool,) =
        sqlx::query_as("SELECT EXISTS(SELECT 1 FROM messages_errors WHERE meta_message_id = $1)")
            .bind(meta_message_id)
            .fetch_one(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
    Ok(failed)
}

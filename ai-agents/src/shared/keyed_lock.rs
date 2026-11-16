use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Mutex};

use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

/// Exclusión mutua por clave: dos tareas con la misma clave se ejecutan
/// una después de la otra; con claves distintas, en paralelo.
///
/// Las entradas sin dueño ni tareas en espera se limpian solas en cada
/// `lock`, así el mapa no crece con cada clave que se haya usado.
pub struct KeyedLocks<K> {
    entries: Mutex<HashMap<K, Arc<AsyncMutex<()>>>>,
}

/// Mientras exista, la clave queda tomada.
pub struct KeyedGuard {
    _guard: OwnedMutexGuard<()>,
}

impl<K: Eq + Hash + Clone> KeyedLocks<K> {
    pub fn new() -> Self {
        Self { entries: Mutex::new(HashMap::new()) }
    }

    /// Espera a que la clave esté libre y la toma.
    pub async fn lock(&self, key: &K) -> KeyedGuard {
        let mutex = {
            let mut entries = self.entries.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            // Solo el mapa tiene la referencia: nadie la tiene ni la espera.
            entries.retain(|_, mutex| Arc::strong_count(mutex) > 1);
            entries.entry(key.clone()).or_default().clone()
        };
        KeyedGuard { _guard: mutex.lock_owned().await }
    }

    /// Claves con dueño o tareas en espera (más las que aún no se limpian).
    pub fn tracked_keys(&self) -> usize {
        self.entries.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).len()
    }
}

impl<K: Eq + Hash + Clone> Default for KeyedLocks<K> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn same_key_is_exclusive_and_other_keys_are_not() {
        let locks = Arc::new(KeyedLocks::<u32>::new());
        let guard = locks.lock(&1).await;

        // Otra clave se toma de inmediato.
        let other = tokio::time::timeout(Duration::from_millis(50), locks.lock(&2)).await;
        assert!(other.is_ok());

        // La misma clave espera hasta que se suelte.
        let waiting = {
            let locks = locks.clone();
            tokio::spawn(async move { locks.lock(&1).await })
        };
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(!waiting.is_finished());
        drop(guard);
        assert!(tokio::time::timeout(Duration::from_millis(200), waiting).await.is_ok());
    }

    #[tokio::test]
    async fn released_keys_are_pruned() {
        let locks = KeyedLocks::<u32>::new();
        for key in 0..10 {
            drop(locks.lock(&key).await);
        }
        let _guard = locks.lock(&99).await;
        assert_eq!(locks.tracked_keys(), 1);
    }
}

use std::sync::Arc;

use async_trait::async_trait;

use crate::quota::dtos::{
    DEFAULT_RESERVATION_MINUTES, QuotaDecision, QuotaLimits, QuotaStatus, ReleaseOutcome, ReserveQuotaDto,
};
use crate::quota::repository::QuotaRepositoryTrait;

#[async_trait]
pub trait QuotaServiceTrait: Send + Sync {
    /// Cupo actual de clientes nuevos en 24 h.
    async fn status(&self) -> Result<QuotaStatus, String>;
    /// Aprueba (y reserva) los teléfonos que caben en el cupo.
    async fn reserve(&self, dto: ReserveQuotaDto) -> Result<QuotaDecision, String>;
    /// Libera una reserva al terminar el envío.
    async fn release(&self, reservation_id: i32) -> Result<ReleaseOutcome, String>;
}

pub struct QuotaService {
    repository: Arc<dyn QuotaRepositoryTrait>,
    limits: QuotaLimits,
}

impl QuotaService {
    pub fn new(repository: Arc<dyn QuotaRepositoryTrait>, limits: QuotaLimits) -> Self {
        Self { repository, limits }
    }
}

#[async_trait]
impl QuotaServiceTrait for QuotaService {
    async fn status(&self) -> Result<QuotaStatus, String> {
        let totals = self.repository.usage().await?;
        Ok(QuotaStatus::new(self.limits, totals))
    }

    async fn reserve(&self, dto: ReserveQuotaDto) -> Result<QuotaDecision, String> {
        let phones = dto.normalized_phones();
        let ttl_minutes = dto.ttl_minutes.unwrap_or(DEFAULT_RESERVATION_MINUTES);
        let decision = self.repository
            .reserve(self.limits, dto.kind, &phones, ttl_minutes, dto.business_id)
            .await?;

        println!(
            "Quota {} (business {:?}): {} of {} phone(s) approved, {} new client(s); \
             used {}, reserved {}, tracking {}+{} of {}, available {} of {}",
            dto.kind.as_str(),
            dto.business_id,
            decision.approved_phones.len(),
            phones.len(),
            decision.new_clients,
            decision.used,
            decision.reserved,
            decision.tracking_used,
            decision.tracking_reserved,
            decision.tracking_limit,
            decision.available,
            decision.limit,
        );
        Ok(decision)
    }

    async fn release(&self, reservation_id: i32) -> Result<ReleaseOutcome, String> {
        self.repository.release(reservation_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quota::dtos::{PhoneQuotaState, QuotaKind, QuotaTotals, allocate};
    use std::sync::Mutex;

    /// Repositorio en memoria: los teléfonos aprobados quedan reservados.
    struct FakeQuotaRepository {
        totals: Mutex<QuotaTotals>,
        reserved_phones: Mutex<Vec<String>>,
        reserve_calls: Mutex<Vec<(QuotaKind, Vec<String>, i32, Option<i32>)>>,
    }

    #[async_trait]
    impl QuotaRepositoryTrait for FakeQuotaRepository {
        async fn usage(&self) -> Result<QuotaTotals, String> {
            Ok(*self.totals.lock().unwrap())
        }

        async fn reserve(
            &self,
            limits: QuotaLimits,
            kind: QuotaKind,
            phones: &[String],
            ttl: i32,
            business_id: Option<i32>,
        ) -> Result<QuotaDecision, String> {
            self.reserve_calls.lock().unwrap().push((kind, phones.to_vec(), ttl, business_id));
            let mut reserved_phones = self.reserved_phones.lock().unwrap();
            let states: Vec<_> = phones
                .iter()
                .map(|phone| PhoneQuotaState {
                    phone: phone.clone(),
                    reserved: reserved_phones.contains(phone),
                    ..Default::default()
                })
                .collect();
            let mut totals = self.totals.lock().unwrap();
            let allocation = allocate(kind, limits, *totals, &states);
            totals.reserved += allocation.reserve.iter().filter(|p| !reserved_phones.contains(p)).count() as i64;
            reserved_phones.extend(allocation.reserve.iter().cloned());
            Ok(QuotaDecision {
                approved: allocation.denied.is_empty(),
                kind,
                approved_phones: allocation.approved,
                denied_phones: allocation.denied,
                new_clients: allocation.new_clients,
                available: allocation.available,
                limit: limits.limit,
                tracking_limit: limits.tracking_limit,
                used: totals.used,
                reserved: totals.reserved,
                tracking_used: totals.tracking_used,
                tracking_reserved: totals.tracking_reserved,
                reservation_id: (!allocation.reserve.is_empty()).then_some(1),
                expires_at: None,
            })
        }

        async fn release(&self, _: i32) -> Result<ReleaseOutcome, String> {
            Ok(ReleaseOutcome::Released)
        }
    }

    const LIMITS: QuotaLimits = QuotaLimits { limit: 230, tracking_limit: 80 };

    fn service(used: i64) -> (QuotaService, Arc<FakeQuotaRepository>) {
        let repository = Arc::new(FakeQuotaRepository {
            totals: Mutex::new(QuotaTotals { used, ..Default::default() }),
            reserved_phones: Mutex::new(Vec::new()),
            reserve_calls: Mutex::new(Vec::new()),
        });
        (QuotaService::new(repository.clone(), LIMITS), repository)
    }

    fn request(phones: &[&str]) -> ReserveQuotaDto {
        ReserveQuotaDto {
            phones: phones.iter().map(|p| p.to_string()).collect(),
            kind: QuotaKind::Office,
            business_id: Some(2),
            ttl_minutes: None,
        }
    }

    #[tokio::test]
    async fn reserves_normalized_phones_with_the_limits_and_default_ttl() {
        let (service, repository) = service(100);

        let decision = service.reserve(request(&["+573001112233", "573001112233", "50251319585"])).await.unwrap();

        assert!(decision.approved);
        assert_eq!(decision.approved_phones, ["573001112233", "50251319585"]);
        assert_eq!(
            repository.reserve_calls.lock().unwrap()[0],
            (QuotaKind::Office, vec!["573001112233".to_string(), "50251319585".to_string()], DEFAULT_RESERVATION_MINUTES, Some(2))
        );
    }

    #[tokio::test]
    async fn a_phone_reserved_by_another_run_does_not_use_quota_twice() {
        let (service, _) = service(229);

        assert!(service.reserve(request(&["573001112233"])).await.unwrap().approved);
        // Cupo agotado, pero ese teléfono ya está reservado: no consume otro.
        let second = service.reserve(request(&["573001112233", "573009998877"])).await.unwrap();
        assert_eq!(second.approved_phones, ["573001112233"]);
        assert_eq!(second.denied_phones, ["573009998877"]);
        assert!(!second.approved);
    }

    #[tokio::test]
    async fn status_reports_the_available_quota_per_kind() {
        let (service, _) = service(200);
        let status = service.status().await.unwrap();
        assert_eq!((status.available_office, status.available_tracking), (30, 30));
    }
}

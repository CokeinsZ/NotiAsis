use async_trait::async_trait;

use crate::payments::dtos::CreatePaymentDto;

#[async_trait]
pub trait PaymentServiceTrait: Send + Sync {
    async fn create_payment(&self, dto: CreatePaymentDto) -> Result<String, String>;
}

pub struct PaymentService {

}

#[async_trait]
impl PaymentServiceTrait for PaymentService {
    async fn create_payment(&self, dto: CreatePaymentDto) -> Result<String, String> {        
        println!("Guardando en base de datos el pago: {}", dto.invoice_id);
        Ok(format!("Pago {} creado con éxito", dto.invoice_id))
    }
}


#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn service_placeholder_test() {
        assert!(true);
    }
}
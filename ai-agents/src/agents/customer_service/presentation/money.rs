/// Formato de cada moneda como lo ven los clientes en las plantillas
/// (`$79.900`, `Q309`).
struct CurrencyFormat {
    symbol: &'static str,
    thousands: char,
    decimals: char,
}

fn format_for(currency: &str) -> Option<CurrencyFormat> {
    match currency.trim().to_uppercase().as_str() {
        "COP" => Some(CurrencyFormat { symbol: "$", thousands: '.', decimals: ',' }),
        "GTQ" => Some(CurrencyFormat { symbol: "Q", thousands: ',', decimals: '.' }),
        _ => None,
    }
}

/// `79900, COP` → `$79.900`; `309, GTQ` → `Q309`; otra moneda → `309 USD`.
/// Sin decimales si el valor es entero.
pub fn format_money(amount: f64, currency: Option<&str>) -> String {
    let currency = currency.unwrap_or("COP");
    let cents = (amount.abs() * 100.0).round() as u64;
    let (units, fraction) = (cents / 100, cents % 100);
    let sign = if amount < 0.0 { "-" } else { "" };

    match format_for(currency) {
        Some(format) => {
            let mut text = format!("{sign}{}{}", format.symbol, group_thousands(units, format.thousands));
            if fraction > 0 {
                text.push(format.decimals);
                text.push_str(&format!("{fraction:02}"));
            }
            text
        }
        None if fraction > 0 => format!("{sign}{units}.{fraction:02} {currency}"),
        None => format!("{sign}{units} {currency}"),
    }
}

fn group_thousands(value: u64, separator: char) -> String {
    let digits = value.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(separator);
        }
        grouped.push(digit);
    }
    grouped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_like_the_whatsapp_templates() {
        assert_eq!(format_money(79_900.0, Some("COP")), "$79.900");
        assert_eq!(format_money(1_139_899.0, Some("cop")), "$1.139.899");
        assert_eq!(format_money(309.0, Some("GTQ")), "Q309");
        assert_eq!(format_money(1_250.5, Some("GTQ")), "Q1,250.50");
        assert_eq!(format_money(0.0, None), "$0");
        assert_eq!(format_money(15.0, Some("USD")), "15 USD");
    }
}

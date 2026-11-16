use chrono::{Datelike, NaiveDate, NaiveDateTime, Timelike, Weekday};

pub fn weekday_name(day: Weekday) -> &'static str {
    match day {
        Weekday::Mon => "lunes",
        Weekday::Tue => "martes",
        Weekday::Wed => "miércoles",
        Weekday::Thu => "jueves",
        Weekday::Fri => "viernes",
        Weekday::Sat => "sábado",
        Weekday::Sun => "domingo",
    }
}

pub fn weekday_abbreviation(day: Weekday) -> &'static str {
    match day {
        Weekday::Mon => "lun",
        Weekday::Tue => "mar",
        Weekday::Wed => "mié",
        Weekday::Thu => "jue",
        Weekday::Fri => "vie",
        Weekday::Sat => "sáb",
        Weekday::Sun => "dom",
    }
}

pub fn month_name(month: u32) -> &'static str {
    const MONTHS: [&str; 12] = [
        "enero",
        "febrero",
        "marzo",
        "abril",
        "mayo",
        "junio",
        "julio",
        "agosto",
        "septiembre",
        "octubre",
        "noviembre",
        "diciembre",
    ];
    MONTHS.get(month.saturating_sub(1) as usize).copied().unwrap_or("")
}

/// `domingo 15 de noviembre de 2026, 3:41 p. m. (2026-11-15)`: el día de
/// la semana ayuda al modelo con "mañana", "el lunes"; la fecha ISO, a
/// calcular fechas.
pub fn describe_moment(moment: NaiveDateTime) -> String {
    let (is_pm, hour) = moment.hour12();
    format!(
        "{} {} de {} de {}, {}:{:02} {} ({})",
        weekday_name(moment.weekday()),
        moment.day(),
        month_name(moment.month()),
        moment.year(),
        hour,
        moment.minute(),
        if is_pm { "p. m." } else { "a. m." },
        moment.format("%Y-%m-%d"),
    )
}

/// Marca corta para el historial: `dom 15/11 13:50`.
pub fn short_stamp(moment: NaiveDateTime) -> String {
    format!("{} {}", weekday_abbreviation(moment.weekday()), moment.format("%d/%m %H:%M"))
}

/// Fecha corta: `lun 16/11`.
pub fn short_date(date: NaiveDate) -> String {
    format!("{} {}", weekday_abbreviation(date.weekday()), date.format("%d/%m"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn moment(hour: u32, minute: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 15).unwrap().and_hms_opt(hour, minute, 0).unwrap()
    }

    #[test]
    fn describes_moments_in_spanish() {
        assert_eq!(describe_moment(moment(15, 41)), "domingo 15 de noviembre de 2026, 3:41 p. m. (2026-11-15)");
        assert_eq!(describe_moment(moment(0, 5)), "domingo 15 de noviembre de 2026, 12:05 a. m. (2026-11-15)");
    }

    #[test]
    fn short_stamp_uses_spanish_weekdays() {
        assert_eq!(short_stamp(moment(13, 50)), "dom 15/11 13:50");
    }
}

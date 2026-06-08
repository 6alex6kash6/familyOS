use chrono::{TimeZone, Utc};
use familyos::csv_import::{CsvImportSource, parse_csv_transactions};
use familyos::integrations::okx::build_okx_history_paths;

#[test]
fn detects_report_months_from_csv_rows() {
    let result = parse_csv_transactions(
        CsvImportSource::Manual,
        "date,amount,currency,category,note\n2026-04-30,-12.50,BYN,Такси,ride\n2026-05-01,-21,BYN,Продукты,shop",
    )
    .unwrap();

    assert_eq!(result.imported_rows, 2);
    assert_eq!(result.detected_months, vec!["2026-04", "2026-05"]);
}

#[test]
fn parses_priorbank_operation_blocks() {
    let csv = [
        "Карточка;ignored",
        "Дата транзакции;Операция;Сумма;Валюта;Дата отражения;Комиссия;Оборот по счету;Цифровая карта;Категория",
        "30.04.2026 12:00:00;YANDEX.GO;6,60;BYN;30.04.2026;0;-6,60;1234;Транспорт - Такси",
        "30.04.2026 13:00:00;Refund;0,01;BYN;30.04.2026;0;0,01;1234;",
        "Всего;;;;",
        "12.05.2026 10:00:00;Outside block;1;BYN;12.05.2026;0;-1;1234;Прочее",
    ]
    .join("\n");

    let result = parse_csv_transactions(CsvImportSource::Priorbank, &csv).unwrap();

    assert_eq!(result.imported_rows, 2);
    assert_eq!(result.detected_months, vec!["2026-04"]);
    assert_eq!(result.rows[0].date, "2026-04-30T12:00:00.000Z");
    assert_eq!(result.rows[0].amount, 6.6);
    assert_eq!(result.rows[0].transaction_type, "expense");
    assert_eq!(result.rows[0].category, "Транспорт - Такси");
    assert_eq!(result.rows[1].transaction_type, "deposit");
}

#[test]
fn builds_monthly_okx_deposit_history_windows_from_beginning_of_year() {
    let paths = build_okx_history_paths(
        "/api/v5/asset/deposit-history",
        Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        Utc.with_ymd_and_hms(2026, 3, 15, 10, 20, 30).unwrap(),
    );

    assert_eq!(
        paths,
        vec![
            "/api/v5/asset/deposit-history?before=1767225600000&after=1769904000000&limit=100",
            "/api/v5/asset/deposit-history?before=1769904000000&after=1772323200000&limit=100",
            "/api/v5/asset/deposit-history?before=1772323200000&after=1773570030000&limit=100",
        ]
    );
}

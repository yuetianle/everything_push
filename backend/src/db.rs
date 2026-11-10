use chrono::{Datelike, Months, NaiveDate, Utc};
#[cfg(test)]
use chrono::DateTime;
use sqlx::{PgPool, pool::PoolOptions};

pub async fn connect(url: &str) -> Result<PgPool, sqlx::Error> {
    PoolOptions::new().max_connections(5).connect(url).await
}

/// 确保当月与下月分区存在（幂等）
pub async fn ensure_partitions(pool: &PgPool) -> Result<(), sqlx::Error> {
    let first = Utc::now().date_naive().with_day(1).unwrap();
    for m in 0u32..=1 {
        let p = first.checked_add_months(Months::new(m)).unwrap();
        ensure_partition_for(pool, p.year(), p.month()).await?;
    }
    Ok(())
}

pub async fn ensure_partition_for(pool: &PgPool, year: i32, month: u32) -> Result<(), sqlx::Error> {
    let start = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    let end = start.checked_add_months(Months::new(1)).unwrap();
    let sql = format!(
        "CREATE TABLE IF NOT EXISTS exchange_rates_{year}{month:02} \
         PARTITION OF exchange_rates FOR VALUES FROM ('{start}') TO ('{end}')"
    );
    sqlx::query(&sql).execute(pool).await?;
    Ok(())
}

/// 删除早于保留窗口的月分区，返回被删除的分区名
pub async fn drop_old_partitions(pool: &PgPool, retain_months: u32) -> Result<Vec<String>, sqlx::Error> {
    let cutoff = Utc::now().date_naive().with_day(1).unwrap()
        .checked_sub_months(Months::new(retain_months)).unwrap();
    // 注意：LIKE 中每个 '_' 都是单字符通配符（包括 "rates" 后面的那个），
    // 分区名 exchange_rates_YYYYMM 共 21 字符，因此需要 7 个 '_' 才能匹配。
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT tablename FROM pg_tables WHERE tablename LIKE 'exchange_rates_______'")
        .fetch_all(pool).await?;
    let mut dropped = vec![];
    for (t,) in rows {
        let ym = &t["exchange_rates_".len()..];
        let (y, m) = (ym[..4].parse::<i32>().unwrap(), ym[4..].parse::<u32>().unwrap());
        if NaiveDate::from_ymd_opt(y, m, 1).unwrap() < cutoff {
            sqlx::query(&format!("DROP TABLE IF EXISTS {t}")).execute(pool).await?;
            dropped.push(t);
        }
    }
    Ok(dropped)
}

/// 仅供集成测试：插入一条汇率
#[cfg(test)]
#[allow(dead_code)]
pub async fn _test_insert(pool: &PgPool, at: DateTime<Utc>) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO exchange_rates (source, pair, mid, recorded_at) VALUES ('api','USD/CNY', 7.1, $1)")
        .bind(at).execute(pool).await?;
    Ok(())
}

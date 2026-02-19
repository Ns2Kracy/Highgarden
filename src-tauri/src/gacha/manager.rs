use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ─── Public types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GachaRecord {
    pub id: String,
    pub uid: String,
    pub game_id: String,
    pub pool_name: String,
    pub pool_type: String, // "standard" | "limited" | "beginner" | "special"
    pub item_name: String,
    pub item_type: String, // "character" | "weapon"
    pub rarity: u8,        // 1–6
    pub timestamp: i64,
    pub is_new: bool,
    pub pity: u32, // pulls since last 6★ in this pool_type
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GachaData {
    pub uid: String,
    pub game_id: String,
    pub records: Vec<GachaRecord>,
    pub fetched_at: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolStats {
    pub pool_type: String,
    pub total_pulls: u32,
    pub six_star_count: u32,
    pub five_star_count: u32,
    pub four_star_count: u32,
    pub three_star_count: u32,
    pub six_star_rate: f64,
    pub current_pity: u32,
    pub avg_pity: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GachaStatsResult {
    pub uid: String,
    pub total_pulls: u32,
    pub by_pool: HashMap<String, PoolStats>,
    pub fetched_at: i64,
}

// ─── Manager ─────────────────────────────────────────────────────────────────

pub struct GachaManager {
    data_dir: PathBuf,
    client: reqwest::Client,
}

impl GachaManager {
    pub fn new(data_dir: PathBuf, client: reqwest::Client) -> Self {
        Self { data_dir, client }
    }

    fn data_path(&self, game_id: &str) -> PathBuf {
        self.data_dir.join(format!("{game_id}_gacha.json"))
    }

    // ── URL scanning ──────────────────────────────────────────────────────────

    /// Scan the game's webCache directories (and AppData LocalLow) for the
    /// gacha-history API URL that contains authentication tokens.
    pub fn scan_gacha_url(&self, game_id: &str, install_path: &str) -> Option<String> {
        let url_pattern = match game_id {
            "arknights" => "ak.hypergryph.com/user/api/inquiry/gacha",
            "endfield" => "beyond.hypergryph.com/user/api/inquiry/gacha",
            _ => return None,
        };

        // 1. Game's webCaches dir (EBWebView / Chromium cache)
        let web_cache = Path::new(install_path).join("webCaches");
        if web_cache.is_dir() {
            if let Some(url) = Self::scan_dir_for_url(&web_cache, url_pattern, 8) {
                return Some(url);
            }
        }

        // 2. data_backup directory
        let data_backup = Path::new(install_path).join("data_backup");
        if data_backup.is_dir() {
            if let Some(url) = Self::scan_dir_for_url(&data_backup, url_pattern, 8) {
                return Some(url);
            }
        }

        // 3. Windows %USERPROFILE%\AppData\LocalLow\Hypergryph\{game}
        if let Ok(profile) = std::env::var("USERPROFILE") {
            let app_name = match game_id {
                "arknights" => "Arknights",
                "endfield" => "Endfield",
                _ => return None,
            };
            let local_low = PathBuf::from(&profile)
                .join("AppData")
                .join("LocalLow")
                .join("Hypergryph")
                .join(app_name);
            if local_low.is_dir() {
                if let Some(url) = Self::scan_dir_for_url(&local_low, url_pattern, 6) {
                    return Some(url);
                }
            }
        }

        None
    }

    fn scan_dir_for_url(dir: &Path, url_pattern: &str, max_depth: u32) -> Option<String> {
        if max_depth == 0 {
            return None;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return None;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(url) = Self::scan_dir_for_url(&path, url_pattern, max_depth - 1) {
                    return Some(url);
                }
            } else {
                // Skip very large files to avoid memory issues
                if let Ok(meta) = entry.metadata() {
                    if meta.len() > 64 * 1024 * 1024 {
                        continue;
                    }
                }
                if let Ok(bytes) = std::fs::read(&path) {
                    if let Some(url) = Self::find_url_in_bytes(&bytes, url_pattern) {
                        return Some(url);
                    }
                }
            }
        }
        None
    }

    fn find_url_in_bytes(bytes: &[u8], url_pattern: &str) -> Option<String> {
        let text = String::from_utf8_lossy(bytes);
        let pattern_pos = text.find(url_pattern)?;
        // Walk back to find https://
        let search_start = pattern_pos.saturating_sub(512);
        let https_offset = text[search_start..pattern_pos].rfind("https://")?;
        let url_start = search_start + https_offset;
        // Walk forward to find end of URL
        let url_end = text[url_start..]
            .find(|c: char| {
                c == '"' || c == '\'' || c == '\n' || c == '\r' || c == '\0' || c == ' '
            })
            .map(|i| url_start + i)
            .unwrap_or_else(|| text.len().min(url_start + 8192));
        let url = text[url_start..url_end].trim().to_string();
        if url.starts_with("https://") && url.contains('?') {
            Some(url)
        } else {
            None
        }
    }

    // ── API fetch (paginated) ─────────────────────────────────────────────────

    /// Fetch all gacha records from the given authenticated URL.
    /// Returns (uid, records_in_chronological_order).
    pub async fn fetch_all_records(
        &self,
        game_id: &str,
        base_url: &str,
    ) -> Result<(String, Vec<GachaRecord>)> {
        // Extract uid from the URL query params
        let uid = extract_query_param(base_url, "uid")
            .or_else(|| extract_query_param(base_url, "channelId").map(|_| String::new()))
            .unwrap_or_default();

        let mut all_entries: Vec<RawEntry> = Vec::new();
        let mut seq_num: i64 = 0; // 0 = start from newest

        loop {
            let url = build_page_url(base_url, seq_num, 10);
            let resp: serde_json::Value = self
                .client
                .get(&url)
                .send()
                .await?
                .json()
                .await?;

            let code = resp
                .get("code")
                .and_then(|c| c.as_i64())
                .unwrap_or(-1);
            if code != 0 {
                let msg = resp
                    .get("msg")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown error");
                return Err(anyhow!("API 返回错误 {code}: {msg}"));
            }

            let data = resp
                .get("data")
                .ok_or_else(|| anyhow!("响应缺少 data 字段"))?;

            // Try to get uid from response if not in URL
            let resp_uid = data
                .get("uid")
                .and_then(|u| u.as_str())
                .map(|s| s.to_string());

            let list = data
                .get("list")
                .and_then(|l| l.as_array())
                .cloned()
                .unwrap_or_default();

            if list.is_empty() {
                break;
            }

            let pagination = data
                .get("pagination")
                .ok_or_else(|| anyhow!("响应缺少 pagination 字段"))?;
            let count = pagination
                .get("count")
                .and_then(|c| c.as_i64())
                .unwrap_or(0);
            let current = pagination
                .get("current")
                .and_then(|c| c.as_i64())
                .unwrap_or(0);

            for entry in &list {
                let ts = entry.get("ts").and_then(|t| t.as_i64()).unwrap_or(0);
                let pool = entry
                    .get("pool")
                    .and_then(|p| p.as_str())
                    .unwrap_or("")
                    .to_string();
                let chars: Vec<RawChar> = entry
                    .get("chars")
                    .and_then(|c| serde_json::from_value(c.clone()).ok())
                    .unwrap_or_default();
                all_entries.push(RawEntry { ts, pool, chars });
            }

            // Try to extract uid from first response
            let final_uid = if uid.is_empty() {
                resp_uid.clone().unwrap_or_default()
            } else {
                uid.clone()
            };

            // Stop when fewer than requested (last page) or no more seqNum
            if count < 10 || current == 0 {
                let records = build_records_with_pity(&final_uid, game_id, all_entries);
                return Ok((final_uid, records));
            }

            seq_num = current;

            // Small delay to avoid rate limiting
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        }

        let final_uid = uid;
        let records = build_records_with_pity(&final_uid, game_id, all_entries);
        Ok((final_uid, records))
    }

    // ── Storage ───────────────────────────────────────────────────────────────

    pub fn load_data(&self, game_id: &str) -> Option<GachaData> {
        let raw = std::fs::read_to_string(self.data_path(game_id)).ok()?;
        serde_json::from_str(&raw).ok()
    }

    pub fn save_data(&self, data: &GachaData) -> Result<()> {
        let path = self.data_path(&data.game_id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, serde_json::to_string_pretty(data)?)?;
        Ok(())
    }

    // ── Statistics ────────────────────────────────────────────────────────────

    pub fn compute_stats(data: &GachaData) -> GachaStatsResult {
        let mut by_pool: HashMap<String, PoolStats> = HashMap::new();

        for record in &data.records {
            let pool = by_pool
                .entry(record.pool_type.clone())
                .or_insert_with(|| PoolStats {
                    pool_type: record.pool_type.clone(),
                    total_pulls: 0,
                    six_star_count: 0,
                    five_star_count: 0,
                    four_star_count: 0,
                    three_star_count: 0,
                    six_star_rate: 0.0,
                    current_pity: 0,
                    avg_pity: 0.0,
                });

            pool.total_pulls += 1;
            match record.rarity {
                6 => pool.six_star_count += 1,
                5 => pool.five_star_count += 1,
                4 => pool.four_star_count += 1,
                _ => pool.three_star_count += 1,
            }
        }

        // Compute derived stats per pool
        for pool_type in by_pool.keys().cloned().collect::<Vec<_>>() {
            let pool_records: Vec<_> = data
                .records
                .iter()
                .filter(|r| r.pool_type == pool_type)
                .collect();

            // Current pity: how many non-6★ pulls from the end
            let current_pity = pool_records
                .iter()
                .rev()
                .take_while(|r| r.rarity < 6)
                .count() as u32;

            // Average pity: mean pulls per 6★
            let mut six_star_pities: Vec<u32> = Vec::new();
            let mut since_last: u32 = 0;
            for r in &pool_records {
                since_last += 1;
                if r.rarity >= 6 {
                    six_star_pities.push(since_last);
                    since_last = 0;
                }
            }
            let avg_pity = if six_star_pities.is_empty() {
                0.0
            } else {
                let sum: u32 = six_star_pities.iter().sum();
                (sum as f64 / six_star_pities.len() as f64 * 10.0).round() / 10.0
            };

            if let Some(pool) = by_pool.get_mut(&pool_type) {
                pool.six_star_rate =
                    (pool.six_star_count as f64 / pool.total_pulls as f64 * 1000.0).round()
                        / 10.0;
                pool.current_pity = current_pity;
                pool.avg_pity = avg_pity;
            }
        }

        GachaStatsResult {
            uid: data.uid.clone(),
            total_pulls: data.records.len() as u32,
            by_pool,
            fetched_at: data.fetched_at,
        }
    }

    // ── Export ────────────────────────────────────────────────────────────────

    pub fn export_json(records: &[GachaRecord], dest_path: &str) -> Result<()> {
        std::fs::write(dest_path, serde_json::to_string_pretty(records)?)?;
        Ok(())
    }

    pub fn export_csv(records: &[GachaRecord], dest_path: &str) -> Result<()> {
        let mut out =
            String::from("\u{FEFF}时间,卡池,卡池类型,干员/物品,稀有度,是否新干员,水位\n");
        for r in records {
            let dt = format_ts(r.timestamp);
            let pool_cn = pool_type_cn(&r.pool_type);
            out.push_str(&format!(
                "{},{},{},{},{},{},{}\n",
                dt,
                csv_escape(&r.pool_name),
                pool_cn,
                csv_escape(&r.item_name),
                r.rarity,
                if r.is_new { "是" } else { "否" },
                r.pity,
            ));
        }
        std::fs::write(dest_path, out.as_bytes())?;
        Ok(())
    }

    pub fn export_xlsx(records: &[GachaRecord], dest_path: &str) -> Result<()> {
        use rust_xlsxwriter::{Format, Workbook};

        let mut workbook = Workbook::new();
        let sheet = workbook.add_worksheet();

        let header_fmt = Format::new().set_bold();
        let headers = [
            "时间",
            "卡池",
            "卡池类型",
            "干员/物品",
            "稀有度",
            "是否新干员",
            "水位",
        ];
        for (col, h) in headers.iter().enumerate() {
            sheet.write_with_format(0, col as u16, *h, &header_fmt)?;
        }

        for (row_idx, r) in records.iter().enumerate() {
            let row = (row_idx + 1) as u32;
            sheet.write(row, 0, format_ts(r.timestamp))?;
            sheet.write(row, 1, r.pool_name.as_str())?;
            sheet.write(row, 2, pool_type_cn(&r.pool_type))?;
            sheet.write(row, 3, r.item_name.as_str())?;
            sheet.write(row, 4, r.rarity as u32)?;
            sheet.write(row, 5, if r.is_new { "是" } else { "否" })?;
            sheet.write(row, 6, r.pity as u32)?;
        }

        sheet.autofit();
        workbook.save(dest_path)?;
        Ok(())
    }
}

// ─── Private helpers ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct RawChar {
    name: String,
    #[serde(rename = "type")]
    item_type: String,
    rarity: u8,
    #[serde(rename = "isNew", default)]
    is_new: bool,
}

struct RawEntry {
    ts: i64,
    pool: String,
    chars: Vec<RawChar>,
}

fn build_records_with_pity(
    uid: &str,
    game_id: &str,
    mut entries: Vec<RawEntry>,
) -> Vec<GachaRecord> {
    // Entries come back newest-first; reverse for chronological order
    entries.reverse();

    let mut pity_counter: HashMap<String, u32> = HashMap::new();
    let mut records = Vec::new();
    let mut global_idx: u32 = 0;

    for entry in entries {
        let pool_type = classify_pool(game_id, &entry.pool).to_string();

        for ch in entry.chars {
            let counter = pity_counter.entry(pool_type.clone()).or_insert(0);
            *counter += 1;
            let pity = *counter;
            let rarity = ch.rarity.saturating_add(1).min(6); // API is 0-indexed (5 = 6★)

            let item_type = if ch.item_type.eq_ignore_ascii_case("CHAR") {
                "character"
            } else {
                "weapon"
            };

            records.push(GachaRecord {
                id: format!("{uid}_{global_idx}"),
                uid: uid.to_string(),
                game_id: game_id.to_string(),
                pool_name: entry.pool.clone(),
                pool_type: pool_type.clone(),
                item_name: ch.name,
                item_type: item_type.to_string(),
                rarity,
                timestamp: entry.ts,
                is_new: ch.is_new,
                pity,
            });

            global_idx += 1;

            if rarity >= 6 {
                *pity_counter.get_mut(&pool_type).unwrap() = 0;
            }
        }
    }

    records
}

fn classify_pool(game_id: &str, pool_name: &str) -> &'static str {
    let name = pool_name.to_lowercase();
    match game_id {
        "arknights" => {
            if name.contains("新手") || name.contains("beginner") {
                "beginner"
            } else if name.contains("标准") || name.contains("standard") {
                "standard"
            } else if name.contains("中坚") || name.contains("veteran") || name.contains("联合") {
                "special"
            } else {
                "limited"
            }
        }
        "endfield" => {
            if name.contains("常驻") || name.contains("standard") || name.contains("标准") {
                "standard"
            } else if name.contains("新手") || name.contains("beginner") {
                "beginner"
            } else {
                "limited"
            }
        }
        _ => "standard",
    }
}

fn build_page_url(base_url: &str, seq_num: i64, size: u32) -> String {
    let sep = if base_url.contains('?') { "&" } else { "?" };
    if seq_num == 0 {
        format!("{base_url}{sep}size={size}")
    } else {
        format!("{base_url}{sep}seqNum={seq_num}&size={size}")
    }
}

fn extract_query_param(url: &str, param: &str) -> Option<String> {
    let query_start = url.find('?')? + 1;
    for part in url[query_start..].split('&') {
        if let Some((k, v)) = part.split_once('=') {
            if k == param && !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

fn format_ts(ts: i64) -> String {
    if ts <= 0 {
        return String::from("1970-01-01 00:00:00");
    }
    let secs = ts as u64;
    let time_of_day = secs % 86400;
    let days = secs / 86400;
    let h = time_of_day / 3600;
    let m = (time_of_day % 3600) / 60;
    let s = time_of_day % 60;
    let (year, month, day) = days_to_ymd(days);
    format!("{year:04}-{month:02}-{day:02} {h:02}:{m:02}:{s:02}")
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    let mut year = 1970u64;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }
    let month_days: [u64; 12] = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 1u64;
    for dm in &month_days {
        if days < *dm {
            break;
        }
        days -= dm;
        month += 1;
    }
    (year, month, days + 1)
}

fn is_leap(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn pool_type_cn(pool_type: &str) -> &'static str {
    match pool_type {
        "standard" => "标准",
        "limited" => "限定",
        "beginner" => "新手",
        "special" => "特殊",
        _ => "其他",
    }
}

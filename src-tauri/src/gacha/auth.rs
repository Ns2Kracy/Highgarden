use anyhow::{anyhow, Result};

// ─── App codes & endpoints ────────────────────────────────────────────────────

pub const AK_APP_CODE: &str = "GzD1CpaWgmSq1wew";
pub const EF_APP_CODE: &str = "6LL0KJuqHBVz33WK";

pub fn app_code_for(game_id: &str) -> &'static str {
    match game_id {
        "endfield" => EF_APP_CODE,
        _ => AK_APP_CODE,
    }
}

pub fn gacha_inquiry_base(game_id: &str) -> &'static str {
    match game_id {
        "endfield" => "https://beyond.hypergryph.com/user/api/inquiry/gacha",
        _ => "https://ak.hypergryph.com/user/api/inquiry/gacha",
    }
}

pub fn build_gacha_url(game_id: &str, grant_token: &str, uid: &str) -> String {
    let base = gacha_inquiry_base(game_id);
    format!("{base}?channelId=1&token={grant_token}&uid={uid}")
}

// ─── Auth API helpers ─────────────────────────────────────────────────────────

/// Login with phone number and password.
/// Returns (uid, token, token_type).
pub async fn login_by_password(
    phone: &str,
    password: &str,
    client: &reqwest::Client,
) -> Result<(String, String, String)> {
    let resp: serde_json::Value = client
        .post("https://as.hypergryph.com/user/auth/v1/token_by_phone_password")
        .json(&serde_json::json!({ "phone": phone, "password": password }))
        .send()
        .await?
        .json()
        .await?;

    check_status(&resp, "密码登录失败")?;

    Ok((
        resp["uid"].as_str().unwrap_or("").to_string(),
        resp["token"].as_str().unwrap_or("").to_string(),
        resp["type"].as_str().unwrap_or("A").to_string(),
    ))
}

/// Send an SMS verification code to the phone number.
pub async fn send_sms_code(phone: &str, client: &reqwest::Client) -> Result<()> {
    let resp: serde_json::Value = client
        .post("https://as.hypergryph.com/general/v1/send_phone_code")
        .json(&serde_json::json!({ "phone": phone, "type": 2 }))
        .send()
        .await?
        .json()
        .await?;

    check_status(&resp, "发送验证码失败")
}

/// Login with phone number and SMS verification code.
/// Returns (uid, token, token_type).
pub async fn login_by_code(
    phone: &str,
    code: &str,
    client: &reqwest::Client,
) -> Result<(String, String, String)> {
    let resp: serde_json::Value = client
        .post("https://as.hypergryph.com/user/auth/v2/token_by_phone_code")
        .json(&serde_json::json!({ "phone": phone, "code": code }))
        .send()
        .await?
        .json()
        .await?;

    check_status(&resp, "验证码登录失败")?;

    Ok((
        resp["uid"].as_str().unwrap_or("").to_string(),
        resp["token"].as_str().unwrap_or("").to_string(),
        resp["type"].as_str().unwrap_or("A").to_string(),
    ))
}

/// Exchange a user auth token for a game-specific grant token.
/// The returned grant token can be passed to build_gacha_url().
pub async fn get_game_grant(
    game_id: &str,
    auth_token: &str,
    client: &reqwest::Client,
) -> Result<String> {
    let resp: serde_json::Value = client
        .post("https://as.hypergryph.com/user/oauth2/v2/grant")
        .json(&serde_json::json!({
            "appCode": app_code_for(game_id),
            "token": auth_token,
            "type": 0
        }))
        .send()
        .await?
        .json()
        .await?;

    check_status(&resp, "获取游戏授权失败")?;

    Ok(resp["content"].as_str().unwrap_or("").to_string())
}

// ─── Utility ──────────────────────────────────────────────────────────────────

fn check_status(resp: &serde_json::Value, default_msg: &str) -> Result<()> {
    let status = resp["status"].as_i64().unwrap_or(-1);
    if status != 0 {
        let msg = resp["msg"]
            .as_str()
            .or_else(|| resp["message"].as_str())
            .unwrap_or(default_msg);
        return Err(anyhow!("{msg}"));
    }
    Ok(())
}

/// Mask a phone number: "13800000000" → "138****0000"
pub fn mask_phone(phone: &str) -> String {
    let digits: String = phone.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() >= 7 {
        format!(
            "{}****{}",
            &digits[..3],
            &digits[digits.len() - 4..]
        )
    } else {
        "***".to_string()
    }
}

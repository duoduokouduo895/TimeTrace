//! 同步数据的端到端加密：云端只存密文，服务商看不到内容。
//!
//! - 密钥：Argon2id(同步口令, 盐)，参数随密文保存，日后可调
//! - 加密：XChaCha20-Poly1305，每次随机 24 字节 nonce；附加数据绑定文件名，防止文件被调包
//! - 口令只保存在本机，不上传、不同步

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use anyhow::{Result, anyhow, bail};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};

const TAG: &str = "timetrace-e2e";
const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct Envelope {
    tt: String,
    v: u32,
    kdf: String,
    /// Argon2 内存（KiB）、迭代次数、并行度
    m: u32,
    t: u32,
    p: u32,
    salt: String,
    nonce: String,
    ct: String,
}

/// 派生一次 Argon2 约几十到几百毫秒，按 (口令指纹, 盐, 参数) 缓存
static KEYS: LazyLock<Mutex<HashMap<String, [u8; 32]>>> = LazyLock::new(Default::default);

fn derive(pass: &str, salt: &[u8], m: u32, t: u32, p: u32) -> Result<[u8; 32]> {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (pass, salt, m, t, p).hash(&mut h);
    let cache_key = format!("{:016x}", h.finish());
    if let Some(k) = KEYS.lock().ok().and_then(|c| c.get(&cache_key).copied()) {
        return Ok(k);
    }
    let params = Params::new(m, t, p, Some(32)).map_err(|e| anyhow!("密钥参数无效：{e}"))?;
    let mut key = [0u8; 32];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(pass.as_bytes(), salt, &mut key)
        .map_err(|e| anyhow!("派生密钥失败：{e}"))?;
    if let Ok(mut c) = KEYS.lock() {
        c.insert(cache_key, key);
    }
    Ok(key)
}

pub fn random_salt() -> Result<String> {
    let mut salt = [0u8; 16];
    getrandom::getrandom(&mut salt).map_err(|e| anyhow!("生成随机数失败：{e}"))?;
    Ok(B64.encode(salt))
}

/// 加密；aad 为文件名，解密时必须一致
pub fn encrypt(plain: &str, pass: &str, salt_b64: &str, aad: &str) -> Result<String> {
    let salt = B64.decode(salt_b64)?;
    let (m, t, p) = (Params::DEFAULT_M_COST, Params::DEFAULT_T_COST, Params::DEFAULT_P_COST);
    let key = derive(pass, &salt, m, t, p)?;
    let mut nonce = [0u8; 24];
    getrandom::getrandom(&mut nonce).map_err(|e| anyhow!("生成随机数失败：{e}"))?;
    let ct = XChaCha20Poly1305::new(&key.into())
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: plain.as_bytes(), aad: aad.as_bytes() })
        .map_err(|_| anyhow!("加密失败"))?;
    Ok(serde_json::to_string(&Envelope {
        tt: TAG.into(),
        v: VERSION,
        kdf: "argon2id".into(),
        m,
        t,
        p,
        salt: salt_b64.into(),
        nonce: B64.encode(nonce),
        ct: B64.encode(ct),
    })?)
}

/// 解密。旧版本上传的明文快照（不是加密信封）原样返回，便于平滑升级。
pub fn decrypt(body: &str, pass: &str, aad: &str) -> Result<String> {
    let env: Envelope = match serde_json::from_str::<Envelope>(body) {
        Ok(e) if e.tt == TAG => e,
        _ => return Ok(body.to_string()),
    };
    if env.v > VERSION {
        bail!("其他设备的 TimeTrace 版本较新，请先升级本机");
    }
    let key = derive(pass, &B64.decode(&env.salt)?, env.m, env.t, env.p)?;
    let nonce = B64.decode(&env.nonce)?;
    if nonce.len() != 24 {
        bail!("密文格式不对");
    }
    let plain = XChaCha20Poly1305::new(&key.into())
        .decrypt(XNonce::from_slice(&nonce), Payload { msg: &B64.decode(&env.ct)?, aad: aad.as_bytes() })
        .map_err(|_| anyhow!("解不开其他设备的数据：同步口令和那台设备不一致（或文件被改动过）"))?;
    Ok(String::from_utf8(plain)?)
}

/// 是否为加密信封（测试用）
#[cfg(test)]
pub fn is_encrypted(body: &str) -> bool {
    serde_json::from_str::<Envelope>(body).is_ok_and(|e| e.tt == TAG)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_failures() {
        let salt = random_salt().unwrap();
        let plain = r#"{"tables":{"activity":[{"name":"背单词"}]}}"#;
        let c1 = encrypt(plain, "口令 123", &salt, "a.json").unwrap();
        let c2 = encrypt(plain, "口令 123", &salt, "a.json").unwrap();
        assert!(is_encrypted(&c1));
        assert_ne!(c1, c2, "随机 nonce：同样内容每次密文不同");
        assert!(!c1.contains("背单词") && !c1.contains("activity"), "密文里不应出现明文");
        assert_eq!(decrypt(&c1, "口令 123", "a.json").unwrap(), plain);
        // 口令错误、文件被调包、内容被篡改都要失败
        assert!(decrypt(&c1, "口令 124", "a.json").unwrap_err().to_string().contains("同步口令"));
        assert!(decrypt(&c1, "口令 123", "b.json").is_err());
        let mut env: serde_json::Value = serde_json::from_str(&c1).unwrap();
        let ct = env["ct"].as_str().unwrap().to_string();
        let flipped = if ct.starts_with('A') { ct.replacen('A', "B", 1) } else { format!("A{}", &ct[1..]) };
        env["ct"] = serde_json::json!(flipped);
        assert!(decrypt(&env.to_string(), "口令 123", "a.json").is_err());
        // 旧版明文原样通过
        assert_eq!(decrypt(plain, "随便", "a.json").unwrap(), plain);
    }
}

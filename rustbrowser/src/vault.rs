//! ORVYN PASSWORDS: локальный сейф.
//!
//! Пароли лежат в `%APPDATA%\Orvyn\vault.json` в зашифрованном виде.
//! Ключ получается из мастер-пароля через Argon2id, шифрование — AES-256-GCM.
//! Мастер-пароль и ключ никогда не пишутся на диск.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use zeroize::{Zeroize, Zeroizing};

use crate::storage;

const VERSION: u32 = 1;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const M_COST: u32 = 19_456;
const T_COST: u32 = 2;
const P_COST: u32 = 1;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Credential {
    pub id: u64,
    pub origin: String,
    pub title: String,
    pub username: String,
    pub password: String,
    pub updated: u64,
}

impl Default for Credential {
    fn default() -> Self {
        Self {
            id: 0,
            origin: String::new(),
            title: String::new(),
            username: String::new(),
            password: String::new(),
            updated: 0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
struct VaultData {
    entries: Vec<Credential>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
struct Envelope {
    version: u32,
    salt: String,
    nonce: String,
    cipher: String,
    m: u32,
    t: u32,
    p: u32,
}

impl Default for Envelope {
    fn default() -> Self {
        Self {
            version: VERSION,
            salt: String::new(),
            nonce: String::new(),
            cipher: String::new(),
            m: M_COST,
            t: T_COST,
            p: P_COST,
        }
    }
}

/// Открытый сейф. Живёт в памяти только после ввода мастер-пароля.
pub struct Vault {
    pub entries: Vec<Credential>,
    salt: Vec<u8>,
    m: u32,
    t: u32,
    p: u32,
    key: Zeroizing<[u8; 32]>,
}

impl Drop for Vault {
    fn drop(&mut self) {
        for e in &mut self.entries {
            e.password.zeroize();
        }
        self.entries.clear();
    }
}

pub fn path() -> PathBuf {
    storage::data_dir().join("vault.json")
}

pub fn exists() -> bool {
    path().exists()
}

pub fn remove() -> Result<(), String> {
    match std::fs::remove_file(path()) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("не удалось удалить сейф: {e}")),
    }
}

/// Создаёт новый пустой сейф и сразу сохраняет его.
pub fn create(master: &str) -> Result<Vault, String> {
    create_at(&path(), master)
}

/// Открывает сейф мастер-паролем.
pub fn open(master: &str) -> Result<Vault, String> {
    open_at(&path(), master)
}

pub fn create_at(file: &Path, master: &str) -> Result<Vault, String> {
    if master.chars().count() < 8 {
        return Err("мастер-пароль должен быть не короче 8 символов".to_string());
    }
    let mut salt = vec![0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);
    let key = derive(master, &salt, M_COST, T_COST, P_COST)?;
    let vault = Vault {
        entries: Vec::new(),
        salt,
        m: M_COST,
        t: T_COST,
        p: P_COST,
        key: Zeroizing::new(key),
    };
    vault.save_at(file)?;
    Ok(vault)
}

pub fn open_at(file: &Path, master: &str) -> Result<Vault, String> {
    let raw = std::fs::read_to_string(file).map_err(|e| format!("не удалось открыть сейф: {e}"))?;
    let env: Envelope =
        serde_json::from_str(&raw).map_err(|_| "это не файл сейфа ORVYN".to_string())?;
    if env.version != VERSION {
        return Err("сейф создан другой версией ORVYN".to_string());
    }
    let salt = STANDARD
        .decode(env.salt.as_bytes())
        .map_err(|_| "повреждён сейф: salt".to_string())?;
    let nonce = STANDARD
        .decode(env.nonce.as_bytes())
        .map_err(|_| "повреждён сейф: nonce".to_string())?;
    let cipher_text = STANDARD
        .decode(env.cipher.as_bytes())
        .map_err(|_| "повреждён сейф: данные".to_string())?;
    if salt.len() < 8 || nonce.len() != NONCE_LEN || cipher_text.len() < 16 {
        return Err("повреждён сейф".to_string());
    }
    check_params(env.m, env.t, env.p)?;
    let mut key = derive(master, &salt, env.m, env.t, env.p)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| "ошибка шифрования".to_string())?;
    let plain = match cipher.decrypt(Nonce::from_slice(&nonce), cipher_text.as_slice()) {
        Ok(v) => Zeroizing::new(v),
        Err(_) => {
            key.zeroize();
            return Err("неверный мастер-пароль".to_string());
        }
    };
    let data: VaultData = serde_json::from_slice(&plain).map_err(|_| "сейф повреждён".to_string())?;
    Ok(Vault {
        entries: data.entries,
        salt,
        m: env.m,
        t: env.t,
        p: env.p,
        key: Zeroizing::new(key),
    })
}

impl Vault {
    pub fn save(&self) -> Result<(), String> {
        self.save_at(&path())
    }

    pub fn save_at(&self, file: &Path) -> Result<(), String> {
        let data = VaultData {
            entries: self.entries.clone(),
        };
        let plain = Zeroizing::new(
            serde_json::to_vec(&data).map_err(|e| format!("не удалось собрать сейф: {e}"))?,
        );
        let mut nonce = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce);
        let cipher = Aes256Gcm::new_from_slice(&*self.key)
            .map_err(|_| "ошибка шифрования".to_string())?;
        let cipher_text = cipher
            .encrypt(Nonce::from_slice(&nonce), plain.as_slice())
            .map_err(|_| "не удалось зашифровать сейф".to_string())?;
        let env = Envelope {
            version: VERSION,
            salt: STANDARD.encode(&self.salt),
            nonce: STANDARD.encode(nonce),
            cipher: STANDARD.encode(cipher_text),
            m: self.m,
            t: self.t,
            p: self.p,
        };
        let text = serde_json::to_string_pretty(&env)
            .map_err(|e| format!("не удалось сохранить сейф: {e}"))?;
        write_atomic(file, text.as_bytes())
    }

    pub fn remove_credential(&mut self, id: u64) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.id != id);
        self.entries.len() != before
    }

    pub fn upsert(&mut self, mut entry: Credential) -> u64 {
        if entry.id == 0 {
            entry.id = self.entries.iter().map(|e| e.id).max().unwrap_or(0) + 1;
        }
        let id = entry.id;
        entry.updated = storage::now();
        if let Some(old) = self.entries.iter_mut().find(|e| e.id == id) {
            *old = entry;
        } else {
            self.entries.push(entry);
        }
        id
    }}

/// Проверяет, что файл похож на сейф ORVYN, не расшифровывая его.
pub fn validate_file(file: &Path) -> Result<(), String> {
    let raw = std::fs::read_to_string(file).map_err(|e| format!("не удалось прочитать файл: {e}"))?;
    let env: Envelope =
        serde_json::from_str(&raw).map_err(|_| "это не файл сейфа ORVYN".to_string())?;
    if env.version != VERSION {
        return Err("сейф создан другой версией ORVYN".to_string());
    }
    let salt = STANDARD
        .decode(env.salt.as_bytes())
        .map_err(|_| "повреждён сейф: salt".to_string())?;
    let nonce = STANDARD
        .decode(env.nonce.as_bytes())
        .map_err(|_| "повреждён сейф: nonce".to_string())?;
    let data = STANDARD
        .decode(env.cipher.as_bytes())
        .map_err(|_| "повреждён сейф: данные".to_string())?;
    if salt.len() < 8 || nonce.len() != NONCE_LEN || data.len() < 16 {
        return Err("повреждён сейф".to_string());
    }
    check_params(env.m, env.t, env.p)
}

/// Устанавливает чужой сейф на место текущего (импорт).
pub fn install_file(src: &Path) -> Result<(), String> {
    validate_file(src)?;
    let bytes = std::fs::read(src).map_err(|e| format!("не удалось прочитать файл: {e}"))?;
    write_atomic(&path(), &bytes)
}

/// Приводит введённый адрес к виду `scheme://host[:port]`.
pub fn normalize_origin(input: &str) -> String {
    let s = input.trim();
    if s.is_empty() {
        return String::new();
    }
    let with_scheme = if s.contains("://") {
        s.to_string()
    } else {
        format!("https://{s}")
    };
    crate::security::origin_of(&with_scheme).unwrap_or_default()
}

fn derive(master: &str, salt: &[u8], m: u32, t: u32, p: u32) -> Result<[u8; 32], String> {
    let params = Params::new(m, t, p, Some(32))
        .map_err(|_| "неверные параметры шифрования".to_string())?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; 32];
    argon
        .hash_password_into(master.as_bytes(), salt, &mut key)
        .map_err(|_| "не удалось получить ключ из мастер-пароля".to_string())?;
    Ok(key)
}

fn check_params(m: u32, t: u32, p: u32) -> Result<(), String> {
    if !(8_192..=262_144).contains(&m) || !(1..=10).contains(&t) || !(1..=4).contains(&p) {
        return Err("неверные параметры сейфа".to_string());
    }
    Ok(())
}

fn write_atomic(file: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = file.with_extension("json.tmp");
    std::fs::write(&tmp, bytes).map_err(|e| format!("не удалось записать сейф: {e}"))?;
    if std::fs::rename(&tmp, file).is_err() {
        std::fs::copy(&tmp, file).map_err(|e| format!("не удалось записать сейф: {e}"))?;
        let _ = std::fs::remove_file(&tmp);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("orvyn-vault-test-{}-{name}.json", std::process::id()));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn vault_roundtrip_and_wrong_password() {
        let p = temp_file("roundtrip");
        let mut v = create_at(&p, "master-pass-123").unwrap();
        v.upsert(Credential {
            id: 0,
            origin: "https://example.com".to_string(),
            title: "Example".to_string(),
            username: "user@example.com".to_string(),
            password: "s3cret".to_string(),
            updated: 0,
        });
        v.save_at(&p).unwrap();
        drop(v);

        let text = std::fs::read_to_string(&p).unwrap();
        assert!(!text.contains("user@example.com"));
        assert!(!text.contains("s3cret"));

        assert!(open_at(&p, "wrong-password").is_err());
        let v = open_at(&p, "master-pass-123").unwrap();
        assert_eq!(v.entries.len(), 1);
        assert_eq!(v.entries[0].password, "s3cret");
        assert_eq!(v.entries[0].origin, "https://example.com");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn origin_normalization() {
        assert_eq!(normalize_origin("example.com"), "https://example.com");
        assert_eq!(
            normalize_origin("http://Example.COM:8080/path?q=1"),
            "http://example.com:8080"
        );
        assert_eq!(normalize_origin("  "), "");
        assert_eq!(normalize_origin("file:///C:/x"), "");
    }
}

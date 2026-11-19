//! In-memory access / refresh tokens and challenge store.

use {
    chrono::Utc,
    dashmap::DashMap,
    log::debug,
    protos::auth::{Role, Token},
    prost_types::Timestamp,
    solana_pubkey::Pubkey,
    std::{sync::Arc, time::Duration},
    uuid::Uuid,
};

#[derive(Clone, Debug)]
pub struct AuthContext {
    pub pubkey: Pubkey,
    pub role: Role,
}

#[derive(Clone)]
struct ChallengeEntry {
    pubkey: Pubkey,
    role: Role,
    expires_at: i64,
}

#[derive(Clone)]
struct IssuedToken {
    ctx: AuthContext,
    expires_at: i64,
}

pub struct TokenStore {
    challenges: DashMap<String, ChallengeEntry>,
    access: DashMap<String, IssuedToken>,
    refresh: DashMap<String, IssuedToken>,
    challenge_ttl: Duration,
    access_ttl: Duration,
    refresh_ttl: Duration,
}

impl TokenStore {
    pub fn new(challenge_ttl: Duration, access_ttl: Duration, refresh_ttl: Duration) -> Self {
        Self {
            challenges: DashMap::new(),
            access: DashMap::new(),
            refresh: DashMap::new(),
            challenge_ttl,
            access_ttl,
            refresh_ttl,
        }
    }

    pub fn put_challenge(&self, challenge: String, pubkey: Pubkey, role: Role) {
        let expires_at = Utc::now().timestamp() + self.challenge_ttl.as_secs() as i64;
        self.challenges.insert(
            challenge,
            ChallengeEntry {
                pubkey,
                role,
                expires_at,
            },
        );
    }

    pub fn take_challenge(&self, challenge: &str) -> Option<(Pubkey, Role)> {
        let (_, entry) = self.challenges.remove(challenge)?;
        if Utc::now().timestamp() > entry.expires_at {
            return None;
        }
        Some((entry.pubkey, entry.role))
    }

    pub fn issue_tokens(&self, pubkey: Pubkey, role: Role) -> (Token, Token) {
        let ctx = AuthContext { pubkey, role };
        let access = self.mint(&self.access, ctx.clone(), self.access_ttl);
        let refresh = self.mint(&self.refresh, ctx, self.refresh_ttl);
        (access, refresh)
    }

    pub fn refresh_access(&self, refresh_token: &str) -> Option<Token> {
        let entry = self.refresh.get(refresh_token)?;
        if Utc::now().timestamp() > entry.expires_at {
            return None;
        }
        let ctx = entry.ctx.clone();
        drop(entry);
        Some(self.mint(&self.access, ctx, self.access_ttl))
    }

    pub fn authorize(&self, bearer: &str) -> Option<AuthContext> {
        let token = bearer.strip_prefix("Bearer ").unwrap_or(bearer);
        let entry = self.access.get(token)?;
        if Utc::now().timestamp() > entry.expires_at {
            return None;
        }
        Some(entry.ctx.clone())
    }

    /// Drop expired challenges and tokens. Returns how many entries were removed.
    pub fn purge_expired(&self) -> usize {
        let now = Utc::now().timestamp();
        let before = self.challenges.len() + self.access.len() + self.refresh.len();
        self.challenges.retain(|_, e| e.expires_at >= now);
        self.access.retain(|_, t| t.expires_at >= now);
        self.refresh.retain(|_, t| t.expires_at >= now);
        before - (self.challenges.len() + self.access.len() + self.refresh.len())
    }

    /// Spawn a background task that calls [`Self::purge_expired`] every `every`.
    /// Must be called from inside a tokio runtime.
    pub fn spawn_purge_task(self: &Arc<Self>, every: Duration) {
        let store = Arc::downgrade(self);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(every);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tick.tick().await;
                let Some(store) = store.upgrade() else { break };
                let removed = store.purge_expired();
                if removed > 0 {
                    debug!("TokenStore: purged {removed} expired challenges/tokens");
                }
            }
        });
    }

    fn mint(
        &self,
        map: &DashMap<String, IssuedToken>,
        ctx: AuthContext,
        ttl: Duration,
    ) -> Token {
        let value = Uuid::new_v4().to_string();
        let expires_at = Utc::now().timestamp() + ttl.as_secs() as i64;
        map.insert(
            value.clone(),
            IssuedToken {
                ctx,
                expires_at,
            },
        );
        Token {
            value,
            expires_at_utc: Some(Timestamp {
                seconds: expires_at,
                nanos: 0,
            }),
        }
    }
}

/// How often the samples sweep expired challenges/tokens out of memory.
pub const PURGE_INTERVAL: Duration = Duration::from_secs(60);

/// Convenience TTLs used by the samples.
pub fn default_ttls() -> (Duration, Duration, Duration) {
    (
        Duration::from_secs(30),
        Duration::from_secs(15 * 60),
        Duration::from_secs(24 * 60 * 60),
    )
}

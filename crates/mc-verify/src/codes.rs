use redis::aio::ConnectionManager;
use uuid::Uuid;

const CODE_MIN: u16 = 1000;
const CODE_MAX: u16 = 9999;
const MAX_ATTEMPTS: usize = 100;

#[derive(Clone)]
pub struct CodeStore {
    redis: ConnectionManager,
}

impl CodeStore {
    pub fn new(redis: ConnectionManager) -> Self {
        Self { redis }
    }

    pub async fn insert(&self, uuid: Uuid, username: String) -> String {
        let mut conn = self.redis.clone();

        for _ in 0..MAX_ATTEMPTS {
            let code = generate_code();
            let stored = coral_redis::verify::store_code(&mut conn, &code, uuid, &username)
                .await
                .expect("failed to store verification code");

            if stored {
                return code;
            }
        }

        panic!("failed to generate unique code after {MAX_ATTEMPTS} attempts");
    }
}

fn generate_code() -> String {
    let n: u16 = CODE_MIN + (rand::random::<u16>() % (CODE_MAX - CODE_MIN + 1));
    n.to_string()
}

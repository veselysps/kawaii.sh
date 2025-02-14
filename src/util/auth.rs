use actix_web::{Error, HttpMessage, HttpRequest, web::Data};
use hmac::Hmac;
use jwt::{VerifyWithKey, SignWithKey, RegisteredClaims};
use sha2::Sha256;

use crate::state::State;
use crate::models::MessageResponse;
use crate::models::user::UserData;

/// Generate auth middleware for a UserRole.
/// This implementation will allow the specified role or lower access level roles to access a resource
macro_rules! define_auth {
    ($name:ident, $role_enum:expr) => {
        // Authentication middleware for this role. This will also work for roles at a lower access level
        pub struct $name(pub $crate::models::user::UserData);

        impl actix_web::FromRequest for $name {
            type Error = actix_web::Error;
            type Future = std::pin::Pin<Box<dyn futures::Future<Output = Result<$name, actix_web::Error>>>>;
            type Config = ();

            fn from_request(req: &actix_web::HttpRequest, _: &mut actix_web::dev::Payload) -> Self::Future {
                let req = req.clone();

                Box::pin(async move {
                    let user_data = match $crate::util::auth::get_auth_data(req).await {
                        Ok(user_data) => user_data,
                        Err(err) => return Err(err)
                    };

                    if user_data.role < $role_enum {
                        return Err(actix_web::Error::from($crate::models::MessageResponse::unauthorized_error()))
                    }

                    Ok($name(user_data))
                })
            }
        }
    }
}

/// Get data from user based on request
async fn get_auth_data(req: HttpRequest) -> Result<UserData, actix_web::Error> {
    let state = req.app_data::<Data<State>>().expect("State was not found");

    let jwt_token = match req.cookie("auth-token") {
        Some(jwt_token) => jwt_token,
        // Token could not be found
        None => return Err(Error::from(MessageResponse::unauthorized_error()))
    };

    // Try to verify token
    let claim: RegisteredClaims = match jwt_token.value().verify_with_key(&state.jwt_key) {
        Ok(claim) => claim,
        // Token verification failed
        Err(_) => return Err(Error::from(MessageResponse::unauthorized_error()))
    };

    let user_id = match claim.subject {
        Some(data) => {
            match data.parse() {
                Ok(parsed) => parsed,
                Err(_) => return Err(Error::from(MessageResponse::internal_server_error()))
            }
        },
        None => return Err(Error::from(MessageResponse::internal_server_error()))
    };

    match state.database.get_user_by_id(user_id).await {
        Ok(data) => Ok(data),
        Err(_) => return Err(Error::from(MessageResponse::internal_server_error()))
    }
}

// Auth middleware defines
pub mod middleware {
    use crate::models::user::UserRole;

    define_auth!(User, UserRole::User);
    define_auth!(Admin, UserRole::Admin);
}

// Sign a JWT token and get a string
pub fn create_jwt_string(id: i32, issuer: &str, timestamp: i64, key: &Hmac<Sha256>) -> Result<String, jwt::Error> {
    let claims = RegisteredClaims {
        issuer: Some(issuer.into()),
        subject: Some(id.to_string().into()),
        expiration: Some(timestamp as u64),
        ..Default::default()
    };

    claims.sign_with_key(key)
}

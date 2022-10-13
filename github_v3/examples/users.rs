use github_v3::*;
use serde_derive::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: u32,
    pub login: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub gravatar_id: Option<String>,
    pub html_url: String,
    pub blog: Option<String>,
    #[serde(rename = "type")]
    pub user_type: String,
    pub created_at: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), GHError> {
    let gh = Client::new_from_env();
    let mut users = gh.get().path("users").send().await?.array::<User>();

    while let Some(Ok(user)) = users.next().await {
        println!("User! {user:#?}");
    }
    Ok(())
}

use futures::StreamExt;
use github_v3::*;

#[tokio::main]
async fn main() -> Result<(), GHError> {
    let gh = Client::new_from_env();
    let mut users = gh.get().path("users").send().await?.array::<model::User>();

    while let Some(Ok(user)) = users.next().await {
        println!("User! {:#?}", user);
    }
    Ok(())
}

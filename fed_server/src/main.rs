mod ingest;

#[macro_use] extern crate rocket;

use rocket::fairing::AdHoc;

#[get("/hello/<name>/<age>")]
fn hello(name: &str, age: u8) -> String {
    format!("Hello, {} year old named {}!", age, name)
}

#[rocket::main]
async fn main() -> Result<(), rocket::Error> {
    let _rocket = rocket::build()
        .mount("/hello", routes![hello])
        .launch()
        .await?;

    Ok(())
}

//! Network test: needs a working system resolver. `cargo test -p envio-net
//! --test dns -- --ignored`.

use envio_net::dns::{SystemTxt, TxtAnswer, TxtLookup};

#[tokio::test]
#[ignore = "needs network access to a DNS resolver"]
async fn a_name_without_a_key_record_is_no_record() {
    let answer = SystemTxt::new()
        .txt("brisbane._domainkey.football.example.com")
        .await;
    assert_eq!(answer, TxtAnswer::NoRecord);
}

use envio_store::Db;

fn pair(name: &str, address: &str) -> (String, String) {
    (name.into(), address.into())
}

fn addresses(db: &Db, prefix: &str) -> Vec<String> {
    db.suggest(prefix, 10)
        .unwrap()
        .into_iter()
        .map(|s| s.address)
        .collect()
}

#[test]
fn suggest_matches_address_and_name_words() {
    let db = Db::open_in_memory().unwrap();
    db.remember_addresses(
        &[
            pair("Ada Lovelace", "ada@example.org"),
            pair("Bob Byron", "bob@example.org"),
            pair("", "lovelace@other.org"),
        ],
        100,
    )
    .unwrap();
    assert_eq!(addresses(&db, "ada"), ["ada@example.org"]);
    assert_eq!(
        addresses(&db, "LOVE"),
        ["ada@example.org", "lovelace@other.org"]
    );
    assert_eq!(addresses(&db, "byr"), ["bob@example.org"]);
    assert_eq!(addresses(&db, "bob@"), ["bob@example.org"]);
    // Only the start of the address or of a name word matches.
    assert!(addresses(&db, "vela").is_empty());
    assert!(addresses(&db, "%").is_empty());
}

#[test]
fn suggest_orders_by_count_then_recency() {
    let db = Db::open_in_memory().unwrap();
    db.remember_addresses(&[pair("A", "u1@x.org")], 100)
        .unwrap();
    db.remember_addresses(&[pair("B", "u2@x.org")], 200)
        .unwrap();
    db.remember_addresses(&[pair("C", "u3@x.org")], 300)
        .unwrap();
    db.remember_addresses(&[pair("A", "u1@x.org")], 150)
        .unwrap();
    assert_eq!(addresses(&db, "u"), ["u1@x.org", "u3@x.org", "u2@x.org"]);
    assert_eq!(db.suggest("u", 1).unwrap().len(), 1);
}

#[test]
fn remember_lowercases_and_counts() {
    let db = Db::open_in_memory().unwrap();
    db.remember_addresses(&[pair("Ada", "Ada@Example.ORG")], 100)
        .unwrap();
    db.remember_addresses(&[pair("", "ada@example.org")], 50)
        .unwrap();
    let found = db.suggest("ada", 10).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].address, "ada@example.org");
    assert_eq!(found[0].name, "Ada");
    assert_eq!(found[0].count, 2);
    assert_eq!(found[0].last_seen, 100);
}

#[test]
fn an_empty_prefix_suggests_nothing() {
    let db = Db::open_in_memory().unwrap();
    db.remember_addresses(&[pair("Ada", "ada@example.org")], 1)
        .unwrap();
    assert!(db.suggest("", 10).unwrap().is_empty());
    assert!(db.suggest("  ", 10).unwrap().is_empty());
}

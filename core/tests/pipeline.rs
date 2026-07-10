//! End-to-end pipeline tests: import multiple sources, then verify that
//! normalization, entity de-duplication, cross-source correlation, search,
//! and identity clustering all behave as the design describes.

use crosstrace_core::correlate::*;
use crosstrace_core::ingest::import_bytes;
use crosstrace_core::search::SearchFilters;
use crosstrace_core::{model::EntityKind, Store};

/// Build a store seeded with a contact, an SMS backup, and a Facebook-style
/// message export that all reference the same person + phone number.
fn seeded_store() -> Store {
    let mut store = Store::open_in_memory().unwrap();

    // 1. Contacts (vCard): John Smith + his phone.
    let vcf = "BEGIN:VCARD\nFN:John Smith\nTEL:+1 555 123 4567\nEMAIL:john@example.com\nEND:VCARD\n";
    import_bytes(&mut store, "contacts.vcf", vcf.as_bytes()).unwrap();

    // 2. SMS backup: a text from John's number.
    let sms = r#"<smses count="1">
        <sms address="+15551234567" contact_name="John Smith" date="1700000000000" type="1" body="see you at 8" />
    </smses>"#;
    import_bytes(&mut store, "sms-backup.xml", sms.as_bytes()).unwrap();

    // 3. Facebook messages export mentioning John and his email.
    let fb = r#"{
        "participants":[{"name":"John Smith"},{"name":"Me"}],
        "messages":[
            {"sender_name":"John Smith","timestamp_ms":1700000100000,"content":"ping me at john@example.com"}
        ]
    }"#;
    import_bytes(&mut store, "facebook/messages/inbox.json", fb.as_bytes()).unwrap();

    store
}

#[test]
fn imports_all_sources_and_records() {
    let store = seeded_store();
    let stats = store.stats().unwrap();
    assert_eq!(stats.sources, 3);
    assert_eq!(stats.records, 3);
    // At least: John (person), phone, email.
    assert!(stats.entities >= 3);
}

#[test]
fn phone_number_is_deduped_across_sources() {
    let store = seeded_store();
    let phones = store.list_entities(Some(EntityKind::Phone)).unwrap();
    let john_phone = phones
        .iter()
        .find(|e| e.value == "+15551234567")
        .expect("phone entity exists");
    // The same normalized number appears in the vCard AND the SMS -> 2 records.
    assert_eq!(john_phone.record_count, 2);
}

#[test]
fn universal_search_by_phone_finds_linked_records() {
    let store = seeded_store();
    let hits = store
        .search("+1 (555) 123-4567", &SearchFilters::default())
        .unwrap();
    // Should find the vCard contact and the SMS via the phone entity.
    assert!(hits.len() >= 2, "expected >=2 hits, got {}", hits.len());
    assert!(hits.iter().any(|h| h.matched_via == "phone"));
}

#[test]
fn full_text_search_matches_body() {
    let store = seeded_store();
    let hits = store.search("ping", &SearchFilters::default()).unwrap();
    assert!(hits.iter().any(|h| h.record.body.as_deref() == Some("ping me at john@example.com")));
}

#[test]
fn correlation_links_person_across_platforms() {
    let store = seeded_store();
    let people = store.list_entities(Some(EntityKind::Person)).unwrap();
    let john = people
        .iter()
        .find(|e| e.value == "John Smith")
        .expect("John exists");
    let corr = store.correlate(john.id, 50).unwrap().unwrap();
    // John appears on contacts, sms and facebook platforms.
    assert!(corr.platforms.len() >= 3, "platforms: {:?}", corr.platforms);
    // His phone/email co-occur with him.
    assert!(corr
        .related_entities
        .iter()
        .any(|r| r.entity.kind == "phone" || r.entity.kind == "email"));
}

#[test]
fn identity_clustering_merges_john_identifiers() {
    let store = seeded_store();
    let clusters = store.identity_clusters(2).unwrap();
    // There should be a cluster containing John plus his phone and email.
    let john_cluster = clusters
        .iter()
        .find(|c| c.members.iter().any(|m| m.value == "John Smith"))
        .expect("john cluster");
    assert!(john_cluster.members.len() >= 3, "members: {:?}", john_cluster.members);
    assert!(john_cluster.label.contains("John"));
}

#[test]
fn hub_does_not_collapse_all_identities() {
    // One "hub" email co-occurs (in the same record body) with many distinct
    // people's emails — like an account owner or a mailing list. The hub guard
    // must stop those people from all merging into a single identity.
    let mut store = Store::open_in_memory().unwrap();
    for i in 0..8 {
        // Each record's body pairs the shared hub email with a unique person,
        // so hub@corp.com co-occurs with 8 different person emails.
        let rec = format!(
            r#"[{{"text":"message from hub@corp.com to person{i}@corp.com"}}]"#
        );
        import_bytes(&mut store, &format!("thread{i}.json"), rec.as_bytes()).unwrap();
    }

    // Sanity: the hub genuinely has high co-occurrence degree.
    // With a low hub limit it must not bridge the person emails together.
    let guarded = store.identity_clusters_opts(2, 2).unwrap();
    let biggest_guarded = guarded.iter().map(|c| c.members.len()).max().unwrap_or(0);
    assert!(
        biggest_guarded < 8,
        "hub guard should prevent a mega-cluster, got size {biggest_guarded}"
    );

    // Without the guard (very high limit), the hub WOULD merge everyone —
    // this proves the test is actually exercising the guard, not a no-op.
    let unguarded = store.identity_clusters_opts(2, 100_000).unwrap();
    let biggest_unguarded = unguarded.iter().map(|c| c.members.len()).max().unwrap_or(0);
    assert!(
        biggest_unguarded >= 8,
        "without the guard the hub should collapse all identities, got {biggest_unguarded}"
    );
}

#[test]
fn record_detail_lists_entities_and_related() {
    let store = seeded_store();
    // Find John's contact record via search, then open its detail.
    let hits = store.search("John Smith", &SearchFilters::default()).unwrap();
    let contact = hits
        .iter()
        .find(|h| h.record.platform == "contacts")
        .expect("a contacts record");
    let detail = store.record_detail(contact.record.id, 20).unwrap().unwrap();
    // The contact references John (person), his phone and email.
    assert!(detail.entities.iter().any(|e| e.entity.kind == "phone"));
    assert!(detail.entities.iter().any(|e| e.entity.kind == "email"));
    // Related records: the SMS and Facebook message that share those entities.
    assert!(!detail.related_records.is_empty());
    assert!(detail.related_records.iter().all(|r| r.id != contact.record.id));
}

#[test]
fn timeline_filter_by_time_window() {
    let store = seeded_store();
    // Window that only includes the second event (1700000100).
    let filters = SearchFilters {
        after: Some(1700000050),
        before: Some(1700000200),
        ..Default::default()
    };
    let hits = store.search("", &filters).unwrap();
    assert!(hits.iter().all(|h| {
        let ts = h.record.timestamp.unwrap_or(0);
        ts >= 1700000050 && ts <= 1700000200
    }));
    assert!(!hits.is_empty());
}

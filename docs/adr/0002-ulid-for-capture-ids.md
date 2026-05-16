# ULID for Capture IDs

Capture rows use ULID string primary keys instead of autoincrement integers or UUIDv4. ULIDs are globally unique (so any future multi-device sync or export is unambiguous) and lexicographically sortable by creation time (so "list newest captures" is a plain ORDER BY id with no separate timestamp index). UUIDv7 has the same properties and would be an equivalent choice; ULID was picked for shorter string form and mature crate support. Autoincrement integers were rejected because they would force a rewrite to migrate off if sync or export ever needs globally unique IDs.

//! The per-project vector store (EMB-FR-RFNF, EMB-FR-KNBG, EMB-FR-CZND,
//! EMB-FR-UJLS, EMB-FR-HBLQ, EMB-FR-EAEA).
//!
//! One SQLite file in the repository store. A vector is keyed by the content
//! hash of the chunk it embeds, so a chunk is embedded once whatever layer or
//! branch holds it. A layer entry names a chunk ID with a content hash, or with
//! no hash as a deletion marker. Distances come from `sqlite-vec`.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Once;
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension};

/// EMB-FR-RFNF: the file name of the store in the repository store.
pub const STORE_FILE: &str = "embeddings.sqlite";

/// EMB-FR-UJLS: the schema version of this build. A store with another version
/// is stale and is built again.
pub const SCHEMA_VERSION: &str = "1";

/// EMB-FR-RFBJ: how long an open waits for another process before the store is
/// unavailable for that attempt.
const BUSY_TIMEOUT: Duration = Duration::from_millis(500);

/// What a layer holds (EMB-FR-LGLI, EMB-FR-DCXI, EMB-FR-MGML, EMB-FR-PYCS).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerKind {
    Root,
    Branch,
    /// EMB-FR-MGML: the one layer of a project outside Git.
    Worktree,
    Documents,
}

impl LayerKind {
    fn as_str(self) -> &'static str {
        match self {
            LayerKind::Root => "root",
            LayerKind::Branch => "branch",
            LayerKind::Worktree => "worktree",
            LayerKind::Documents => "documents",
        }
    }

    fn parse(text: &str) -> Option<LayerKind> {
        match text {
            "root" => Some(LayerKind::Root),
            "branch" => Some(LayerKind::Branch),
            "worktree" => Some(LayerKind::Worktree),
            "documents" => Some(LayerKind::Documents),
            _ => None,
        }
    }
}

/// One stored layer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayerRow {
    pub id: i64,
    pub kind: LayerKind,
    /// The root branch for the root layer, the branch for a branch layer.
    pub branch: Option<String>,
    /// EMB-FR-LGLI: the tip commit id the root layer was read from.
    pub tip: Option<String>,
    /// EMB-FR-USBA: the root tip a branch layer was compared with.
    pub root_tip: Option<String>,
}

/// The layer entries: chunk ID to content hash, `None` being a deletion marker.
pub type Entries = BTreeMap<String, Option<String>>;

/// The layers a search sees (EMB-FR-DSUG). `None` reads as an empty layer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct View {
    pub root: Option<i64>,
    pub branch: Option<i64>,
}

pub struct Store {
    conn: Connection,
}

/// Register `sqlite-vec` with every connection this process opens.
fn register_sqlite_vec() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // SAFETY: `sqlite3_vec_init` is the extension entry point that
        // `sqlite3_auto_extension` expects; this is the registration the
        // `sqlite-vec` crate documents. It runs once, before any connection.
        #[allow(clippy::missing_transmute_annotations)]
        unsafe {
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
                sqlite_vec::sqlite3_vec_init as *const (),
            )));
        }
    });
}

/// The store file in the repository store `dir`.
pub fn store_path(dir: &Path) -> PathBuf {
    dir.join(STORE_FILE)
}

impl Store {
    /// Open the store in `dir` for `model_id`, creating it when absent.
    ///
    /// EMB-FR-UJLS: a store of another model or schema version is deleted and
    /// created empty; the second value of the result says so. Every failure is
    /// the reason the store is unavailable (EMB-FR-RFBJ).
    pub fn open(dir: &crate::fs::RootFs, model_id: &str) -> Result<(Store, bool), String> {
        let path = store_path(dir.path());
        let store = Store::connect(&path)?;
        match store.meta("model")? {
            None => {
                store.init(model_id)?;
                Ok((store, false))
            }
            Some(model)
                if model == model_id
                    && store.meta("schema")?.as_deref() == Some(SCHEMA_VERSION) =>
            {
                Ok((store, false))
            }
            Some(_) => {
                drop(store);
                delete_files(dir, &path)?;
                let store = Store::connect(&path)?;
                store.init(model_id)?;
                Ok((store, true))
            }
        }
    }

    /// Open an existing store for a search, with no schema change.
    pub fn open_existing(path: &Path) -> Result<Store, String> {
        Store::connect(path)
    }

    /// FSA-FR-19 exception, deliberate and narrow: SQLite owns its database
    /// file, its journal, and its locks, so it opens the path itself. The path
    /// is the one file of `EMB-FR-RFNF` in the repository store, which the
    /// caller reached through that store's `RootFs`.
    fn connect(path: &Path) -> Result<Store, String> {
        register_sqlite_vec();
        let conn = Connection::open(path).map_err(|e| format!("{UNAVAILABLE}: open: {e}"))?;
        conn.busy_timeout(BUSY_TIMEOUT).map_err(|e| format!("{UNAVAILABLE}: busy timeout: {e}"))?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| format!("{UNAVAILABLE}: journal: {e}"))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )
        .map_err(|e| format!("{UNAVAILABLE}: meta: {e}"))?;
        Ok(Store { conn })
    }

    fn meta(&self, key: &str) -> Result<Option<String>, String> {
        self.conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
            .optional()
            .map_err(|e| format!("{UNAVAILABLE}: read meta: {e}"))
    }

    fn init(&self, model_id: &str) -> Result<(), String> {
        self.conn
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS vectors (
                     hash TEXT PRIMARY KEY,
                     embedding BLOB NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS layers (
                     id INTEGER PRIMARY KEY,
                     kind TEXT NOT NULL,
                     branch TEXT,
                     tip TEXT,
                     root_tip TEXT
                 );
                 CREATE TABLE IF NOT EXISTS entries (
                     layer_id INTEGER NOT NULL,
                     chunk_id TEXT NOT NULL,
                     content_hash TEXT,
                     PRIMARY KEY (layer_id, chunk_id)
                 ) WITHOUT ROWID;
                 CREATE INDEX IF NOT EXISTS entries_hash ON entries (content_hash);",
            )
            .map_err(|e| format!("{UNAVAILABLE}: schema: {e}"))?;
        self.conn
            .execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES ('model', ?1), ('schema', ?2)",
                params![model_id, SCHEMA_VERSION],
            )
            .map_err(|e| format!("{UNAVAILABLE}: write meta: {e}"))?;
        Ok(())
    }

    /// Every stored layer, oldest first.
    pub fn layers(&self) -> Result<Vec<LayerRow>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, kind, branch, tip, root_tip FROM layers ORDER BY id")
            .map_err(|e| format!("{UNAVAILABLE}: layers: {e}"))?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                ))
            })
            .map_err(|e| format!("{UNAVAILABLE}: layers: {e}"))?;
        let mut out = Vec::new();
        for row in rows {
            let (id, kind, branch, tip, root_tip) =
                row.map_err(|e| format!("{UNAVAILABLE}: layers: {e}"))?;
            if let Some(kind) = LayerKind::parse(&kind) {
                out.push(LayerRow { id, kind, branch, tip, root_tip });
            }
        }
        Ok(out)
    }

    /// The layer of `kind` for `branch`, if stored. Root, worktree, and
    /// documents layers are one per store, so their branch is not compared.
    #[cfg(test)]
    pub fn find_layer(
        &self,
        kind: LayerKind,
        branch: Option<&str>,
    ) -> Result<Option<LayerRow>, String> {
        Ok(self.layers()?.into_iter().find(|layer| {
            layer.kind == kind && (kind != LayerKind::Branch || layer.branch.as_deref() == branch)
        }))
    }

    /// The entries of one layer.
    pub fn entries(&self, layer_id: i64) -> Result<Entries, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT chunk_id, content_hash FROM entries WHERE layer_id = ?1")
            .map_err(|e| format!("{UNAVAILABLE}: entries: {e}"))?;
        let rows = stmt
            .query_map([layer_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)))
            .map_err(|e| format!("{UNAVAILABLE}: entries: {e}"))?;
        rows.collect::<Result<Entries, _>>().map_err(|e| format!("{UNAVAILABLE}: entries: {e}"))
    }

    /// The hashes of `hashes` that have no vector yet (EMB-FR-KNBG).
    #[cfg(test)]
    pub fn missing_vectors<'a>(
        &self,
        hashes: impl IntoIterator<Item = &'a String>,
    ) -> Result<Vec<String>, String> {
        let mut stmt = self.conn.prepare("SELECT 1 FROM vectors WHERE hash = ?1").map_err(sql)?;
        let mut missing = Vec::new();
        for hash in hashes {
            if !stmt.exists([hash]).map_err(sql)? {
                missing.push(hash.clone());
            }
        }
        Ok(missing)
    }

    /// EMB-FR-KNBG, EMB-FR-RXCL: the content hashes that a layer outside
    /// `excluded` refers to. Each has a vector, which a pass reuses.
    pub fn hashes_outside(&self, excluded: &[i64]) -> Result<HashSet<String>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT layer_id, content_hash FROM entries WHERE content_hash IS NOT NULL")
            .map_err(sql)?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            .map_err(sql)?;
        let mut out = HashSet::new();
        for row in rows {
            let (layer, hash) = row.map_err(sql)?;
            if !excluded.contains(&layer) {
                out.insert(hash);
            }
        }
        Ok(out)
    }

    /// EMB-FR-EAEA: apply a whole pass in one transaction — delete the
    /// `deleted` layers, store `vectors` (replacing a vector of the same hash),
    /// make each changed layer hold its entries, and remove every vector no
    /// layer refers to any more (EMB-FR-HBLQ). Returns the id of each changed
    /// layer, in order, and how many entries the changes removed.
    pub fn commit(
        &mut self,
        deleted: &[i64],
        changes: &[LayerChange<'_>],
        vectors: &[(String, Vec<f32>)],
    ) -> Result<(Vec<i64>, usize), String> {
        let tx = self.conn.transaction().map_err(sql)?;
        for id in deleted {
            tx.execute("DELETE FROM entries WHERE layer_id = ?1", [id]).map_err(sql)?;
            tx.execute("DELETE FROM layers WHERE id = ?1", [id]).map_err(sql)?;
        }
        {
            let mut insert = tx
                .prepare("INSERT OR REPLACE INTO vectors (hash, embedding) VALUES (?1, ?2)")
                .map_err(sql)?;
            for (hash, vector) in vectors {
                insert.execute(params![hash, vector_blob(vector)]).map_err(sql)?;
            }
        }
        let mut ids = Vec::with_capacity(changes.len());
        let mut removed = 0usize;
        for change in changes {
            let (id, gone) = apply_in(&tx, change)?;
            ids.push(id);
            removed += gone;
        }
        collect_garbage(&tx)?;
        tx.commit().map_err(sql)?;
        Ok((ids, removed))
    }

    /// One layer change in its own transaction, with its vectors.
    #[cfg(test)]
    pub fn apply(
        &mut self,
        change: LayerChange<'_>,
        vectors: &[(String, Vec<f32>)],
    ) -> Result<(i64, usize), String> {
        let (ids, removed) = self.commit(&[], &[change], vectors)?;
        Ok((ids[0], removed))
    }

    /// Delete layers and the vectors that only they referred to.
    #[cfg(test)]
    pub fn delete_layers(&mut self, ids: &[i64]) -> Result<(), String> {
        self.commit(ids, &[], &[]).map(|_| ())
    }

    /// How many vectors the store holds.
    #[cfg(test)]
    pub fn vector_count(&self) -> Result<usize, String> {
        self.conn
            .query_row("SELECT COUNT(*) FROM vectors", [], |r| r.get::<_, i64>(0))
            .map(|n| n as usize)
            .map_err(sql)
    }

    /// EMB-FR-DSUG: the entries of the current view — the root layer without
    /// the chunks the branch layer replaces or deletes, plus the branch chunks.
    pub fn view_entries(&self, view: View) -> Result<BTreeMap<String, String>, String> {
        let mut out: BTreeMap<String, String> = BTreeMap::new();
        let branch = match view.branch {
            Some(id) => self.entries(id)?,
            None => Entries::new(),
        };
        if let Some(root) = view.root {
            for (chunk_id, hash) in self.entries(root)? {
                if let (false, Some(hash)) = (branch.contains_key(&chunk_id), hash) {
                    out.insert(chunk_id, hash);
                }
            }
        }
        for (chunk_id, hash) in branch {
            if let Some(hash) = hash {
                out.insert(chunk_id, hash);
            }
        }
        Ok(out)
    }

    /// EMB-FR-NJYA: the `k` chunks of the current view nearest to `query` by
    /// cosine distance, nearest first, with their content hashes.
    pub fn nearest(
        &self,
        view: View,
        query: &[f32],
        k: usize,
    ) -> Result<Vec<(String, String)>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "WITH b AS (SELECT chunk_id, content_hash FROM entries WHERE layer_id = ?2),
                 v AS (
                     SELECT chunk_id, content_hash FROM entries
                      WHERE layer_id = ?1 AND content_hash IS NOT NULL
                        AND chunk_id NOT IN (SELECT chunk_id FROM b)
                     UNION ALL
                     SELECT chunk_id, content_hash FROM b WHERE content_hash IS NOT NULL
                 )
                 SELECT v.chunk_id, v.content_hash
                   FROM v JOIN vectors x ON x.hash = v.content_hash
                  ORDER BY vec_distance_cosine(x.embedding, ?3), v.chunk_id
                  LIMIT ?4",
            )
            .map_err(sql)?;
        let rows = stmt
            .query_map(
                params![
                    view.root.unwrap_or(-1),
                    view.branch.unwrap_or(-1),
                    vector_blob(query),
                    k as i64
                ],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .map_err(sql)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(sql)
    }
}

/// One layer change for [`Store::commit`].
pub struct LayerChange<'a> {
    /// The stored layer, or `None` to create it.
    pub layer_id: Option<i64>,
    pub kind: LayerKind,
    pub branch: Option<&'a str>,
    pub tip: Option<&'a str>,
    pub root_tip: Option<&'a str>,
    /// The entries the layer holds after the change.
    pub entries: &'a Entries,
}

/// Make one layer hold `change.entries` inside `tx`, creating it when absent.
fn apply_in(
    tx: &rusqlite::Transaction<'_>,
    change: &LayerChange<'_>,
) -> Result<(i64, usize), String> {
    let layer_id = match change.layer_id {
        Some(id) => {
            tx.execute(
                "UPDATE layers SET branch = ?2, tip = ?3, root_tip = ?4 WHERE id = ?1",
                params![id, change.branch, change.tip, change.root_tip],
            )
            .map_err(sql)?;
            id
        }
        None => {
            tx.execute(
                "INSERT INTO layers (kind, branch, tip, root_tip) VALUES (?1, ?2, ?3, ?4)",
                params![change.kind.as_str(), change.branch, change.tip, change.root_tip],
            )
            .map_err(sql)?;
            tx.last_insert_rowid()
        }
    };
    let current: Entries = {
        let mut stmt = tx
            .prepare("SELECT chunk_id, content_hash FROM entries WHERE layer_id = ?1")
            .map_err(sql)?;
        let rows = stmt
            .query_map([layer_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)))
            .map_err(sql)?;
        rows.collect::<Result<Entries, _>>().map_err(sql)?
    };
    let mut removed = 0usize;
    let mut delete =
        tx.prepare("DELETE FROM entries WHERE layer_id = ?1 AND chunk_id = ?2").map_err(sql)?;
    for chunk_id in current.keys().filter(|id| !change.entries.contains_key(*id)) {
        delete.execute(params![layer_id, chunk_id]).map_err(sql)?;
        removed += 1;
    }
    let mut upsert = tx
        .prepare(
            "INSERT OR REPLACE INTO entries (layer_id, chunk_id, content_hash) VALUES (?1, ?2, ?3)",
        )
        .map_err(sql)?;
    for (chunk_id, hash) in change.entries {
        if current.get(chunk_id) != Some(hash) {
            upsert.execute(params![layer_id, chunk_id, hash]).map_err(sql)?;
        }
    }
    Ok((layer_id, removed))
}

fn collect_garbage(tx: &rusqlite::Transaction<'_>) -> Result<(), String> {
    tx.execute(
        "DELETE FROM vectors WHERE hash NOT IN
             (SELECT content_hash FROM entries WHERE content_hash IS NOT NULL)",
        [],
    )
    .map(|_| ())
    .map_err(sql)
}

/// EMB-FR-RFBJ: the start of every error of this store. A store that cannot be
/// opened, read, or written is unavailable, busy with another process included.
pub const UNAVAILABLE: &str = "store unavailable";

fn sql(e: rusqlite::Error) -> String {
    format!("{UNAVAILABLE}: {e}")
}

/// A vector as the little-endian `float32` blob `sqlite-vec` reads.
fn vector_blob(vector: &[f32]) -> Vec<u8> {
    vector.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// EMB-FR-UJLS: delete a stale store with its journal files.
fn delete_files(dir: &crate::fs::RootFs, path: &Path) -> Result<(), String> {
    let mut targets = vec![path.to_path_buf()];
    for suffix in ["-wal", "-shm"] {
        let mut name = path.as_os_str().to_owned();
        name.push(suffix);
        targets.push(PathBuf::from(name));
    }
    for target in targets {
        match dir.access().delete_path(&target, false) {
            Ok(()) | Err(crate::fs::FsError::NotFound { .. }) => {}
            Err(e) => return Err(format!("{UNAVAILABLE}: delete stale store: {e}")),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(pairs: &[(&str, Option<&str>)]) -> Entries {
        pairs.iter().map(|(id, h)| (id.to_string(), h.map(str::to_string))).collect()
    }

    fn open(dir: &tempfile::TempDir, model: &str) -> (Store, bool) {
        Store::open(&crate::fs::RootFs::for_root(dir.path()), model).unwrap()
    }

    fn put(
        store: &mut Store,
        layer: Option<i64>,
        kind: LayerKind,
        e: &Entries,
        v: &[(String, Vec<f32>)],
    ) -> i64 {
        store
            .apply(
                LayerChange {
                    layer_id: layer,
                    kind,
                    branch: None,
                    tip: None,
                    root_tip: None,
                    entries: e,
                },
                v,
            )
            .unwrap()
            .0
    }

    // EMB-FR-KNBG, EMB-FR-CZND, EMB-FR-HBLQ: entries name hashes, a vector is
    // stored once per hash, and the last reference to go takes its vector.
    #[test]
    fn vectors_are_keyed_by_hash_and_collected_when_unreferenced() {
        let dir = tempfile::tempdir().unwrap();
        let (mut store, rebuilt) = open(&dir, "m");
        assert!(!rebuilt);
        let v = vec![("h1".to_string(), vec![1.0, 0.0]), ("h2".to_string(), vec![0.0, 1.0])];
        let e = entries(&[("a.md#0", Some("h1")), ("b.md#0", Some("h2")), ("c.md#0", Some("h1"))]);
        let layer = put(&mut store, None, LayerKind::Root, &e, &v);
        assert_eq!(store.vector_count().unwrap(), 2);
        assert_eq!(store.missing_vectors(&["h1".to_string(), "h3".to_string()]).unwrap(), ["h3"]);

        let e = entries(&[("c.md#0", Some("h1"))]);
        put(&mut store, Some(layer), LayerKind::Root, &e, &[]);
        assert_eq!(store.entries(layer).unwrap(), e);
        assert_eq!(store.vector_count().unwrap(), 1, "h2 has no reference left");
    }

    // EMB-FR-UJLS: a store of another model is deleted and built again empty.
    #[test]
    fn a_store_of_another_model_is_stale() {
        let dir = tempfile::tempdir().unwrap();
        {
            let (mut store, _) = open(&dir, "first");
            let e = entries(&[("a.md#0", Some("h1"))]);
            put(&mut store, None, LayerKind::Root, &e, &[("h1".to_string(), vec![1.0])]);
        }
        let (store, rebuilt) = open(&dir, "first");
        assert!(!rebuilt);
        assert_eq!(store.layers().unwrap().len(), 1);
        drop(store);
        let (store, rebuilt) = open(&dir, "second");
        assert!(rebuilt);
        assert!(store.layers().unwrap().is_empty());
        assert_eq!(store.vector_count().unwrap(), 0);
    }

    // EMB-FR-UJLS: a store of another schema version is stale too.
    #[test]
    fn a_store_of_another_schema_version_is_stale() {
        let dir = tempfile::tempdir().unwrap();
        {
            let (store, _) = open(&dir, "m");
            store.conn.execute("UPDATE meta SET value = '0' WHERE key = 'schema'", []).unwrap();
        }
        let (_, rebuilt) = open(&dir, "m");
        assert!(rebuilt);
    }

    // EMB-FR-DSUG, EMB-FR-NJYA: the view replaces and deletes root chunks, and
    // the nearest query sees only the view.
    #[test]
    fn the_view_lays_the_branch_over_the_root() {
        let dir = tempfile::tempdir().unwrap();
        let (mut store, _) = open(&dir, "m");
        let v = vec![
            ("r1".to_string(), vec![1.0, 0.0]),
            ("r2".to_string(), vec![0.0, 1.0]),
            ("r3".to_string(), vec![0.7, 0.7]),
        ];
        let root = entries(&[("a#0", Some("r1")), ("b#0", Some("r2")), ("c#0", Some("r3"))]);
        let root_id = put(&mut store, None, LayerKind::Root, &root, &v);
        let branch = entries(&[("a#0", Some("b1")), ("c#0", None), ("d#0", Some("r2"))]);
        let branch_id = put(
            &mut store,
            None,
            LayerKind::Branch,
            &branch,
            &[("b1".to_string(), vec![0.9, 0.1])],
        );
        let view = View { root: Some(root_id), branch: Some(branch_id) };
        let seen = store.view_entries(view).unwrap();
        let expected: BTreeMap<String, String> = [("a#0", "b1"), ("b#0", "r2"), ("d#0", "r2")]
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        assert_eq!(seen, expected);

        let nearest = store.nearest(view, &[1.0, 0.0], 10).unwrap();
        let ids: Vec<&str> = nearest.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, ["a#0", "b#0", "d#0"], "c#0 is deleted, a#0 is the branch chunk");
        let root_only =
            store.nearest(View { root: Some(root_id), branch: None }, &[1.0, 0.0], 1).unwrap();
        assert_eq!(root_only, [("a#0".to_string(), "r1".to_string())]);
    }

    // EMB-FR-HRYV, EMB-FR-RXCL: deleting a layer takes the vectors only it
    // referred to and keeps those a kept layer refers to.
    #[test]
    fn deleting_a_layer_keeps_vectors_of_kept_layers() {
        let dir = tempfile::tempdir().unwrap();
        let (mut store, _) = open(&dir, "m");
        let root = put(
            &mut store,
            None,
            LayerKind::Root,
            &entries(&[("a#0", Some("shared"))]),
            &[("shared".to_string(), vec![1.0])],
        );
        let docs = put(
            &mut store,
            None,
            LayerKind::Documents,
            &entries(&[("doc-1#0", Some("shared")), ("doc-1#1", Some("own"))]),
            &[("own".to_string(), vec![0.5])],
        );
        assert_eq!(store.vector_count().unwrap(), 2);
        store.delete_layers(&[docs]).unwrap();
        assert_eq!(store.vector_count().unwrap(), 1, "only the documents layer needed `own`");
        assert_eq!(store.missing_vectors(&["shared".to_string()]).unwrap(), Vec::<String>::new());
        assert_eq!(store.layers().unwrap().iter().map(|l| l.id).collect::<Vec<_>>(), [root]);
    }
}

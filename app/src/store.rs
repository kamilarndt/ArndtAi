//! Magazyn SQLite: sessions / steps / outbox / turns / tasks / transcripts.
//! Plik: `.agent/state.db`. Jeden writer, transakcje rusqlite `bundled`.
//!
//! Runda 11 (M1): `tasks` (zadanie jako wiersz) i `transcripts` (kopia eventow
//! JSONL) plus `transcripts_fts` (FTS5) przeszukiwane na poziomie pamieci `pro`.
//! JSONL zostaje eksportem i kontraktem — baza jest drugim, przeszukiwalnym
//! zapisem tej samej tresci.

use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[derive(Clone, Debug)]
pub struct StepRow {
    #[allow(dead_code)] // nosi tozsamosc kroku dla diagnostyki
    pub step_id: i64,
    pub tool: String,
    pub args_json: String,
    pub status: String,
    pub result_json: Option<String>,
}

#[derive(Clone, Debug)]
pub struct TurnRow {
    pub turn_no: i64,
    pub status: String,
    pub base_step: u64,
}

/// Runda 11: jedno zadanie = jeden wiersz. `status` uzywa tych samych slow co
/// podsumowanie sesji (`running` w trakcie, potem `done`/`failed`).
#[derive(Clone, Debug)]
#[allow(dead_code)] // API diagnostyczne (testy, przyszly `status`)
pub struct TaskRow {
    pub id: String,
    #[allow(dead_code)] // wiazanie zadanie -> sesja (diagnostyka, testy)
    pub session_id: String,
    pub text: String,
    pub status: String,
}

/// Runda 11: jeden wiersz = jedna linia JSONL tej sesji.
#[derive(Clone, Debug)]
#[allow(dead_code)] // API diagnostyczne (testy)
pub struct TranscriptRow {
    #[allow(dead_code)] // tozsamosc wiersza (UPDATE/DELETE w testach)
    pub id: i64,
    pub session_id: String,
    pub step: Option<i64>,
    pub kind: String,
    pub payload: String,
    #[allow(dead_code)] // znacznik czasu eventu (diagnostyka)
    pub at: i64,
}

/// Trafienie FTS: skad fragment i jak wyglada (snippet z FTS5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptHit {
    pub session_id: String,
    pub step: i64,
    pub snippet: String,
}

/// Ile trafien historii wchodzi do kontekstu poziomu `pro`.
pub const HISTORY_HITS: usize = 8;

#[derive(Clone)]
pub struct Store {
    pub path: PathBuf,
}

impl Store {
    pub fn new(db_path: &Path) -> Store {
        Store {
            path: db_path.to_path_buf(),
        }
    }

    pub fn for_workspace(workspace: &Path) -> Store {
        Store::new(&workspace.join(".agent").join("state.db"))
    }

    fn conn(&self) -> Result<Connection, String> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("nie mozna utworzyc .agent: {}", e))?;
        }
        let c = Connection::open(&self.path).map_err(|e| format!("sqlite open: {}", e))?;
        // F6: WAL + busy_timeout. Jeden writer (rdzen), ale procesy MCP czytaja.
        c.busy_timeout(std::time::Duration::from_millis(5000))
            .map_err(|e| format!("sqlite busy_timeout: {}", e))?;
        c.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| format!("sqlite journal_mode: {}", e))?;
        c.pragma_update(None, "synchronous", "NORMAL")
            .map_err(|e| format!("sqlite synchronous: {}", e))?;
        Ok(c)
    }

    /// Diagnostyka: tryb dziennika wymuszony przez `conn`.
    pub fn journal_mode(&self) -> Result<String, String> {
        let c = self.conn()?;
        c.pragma_query_value(None, "journal_mode", |row| row.get::<_, String>(0))
            .map_err(|e| format!("sqlite journal_mode read: {}", e))
    }

    pub fn busy_timeout(&self) -> Result<i64, String> {
        let c = self.conn()?;
        c.pragma_query_value(None, "busy_timeout", |row| row.get::<_, i64>(0))
            .map_err(|e| format!("sqlite busy_timeout read: {}", e))
    }

    /// Tworzy tabele jesli nie istnieja. Idempotentne.
    pub fn init(&self) -> Result<(), String> {
        let c = self.conn()?;
        c.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions(
                 id TEXT PRIMARY KEY,
                 task TEXT NOT NULL,
                 created_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS steps(
                 session_id TEXT NOT NULL,
                 step_id INTEGER NOT NULL,
                 tool TEXT NOT NULL,
                 args_json TEXT NOT NULL,
                 status TEXT NOT NULL,
                 result_json TEXT,
                 created_at INTEGER NOT NULL,
                 PRIMARY KEY(session_id, step_id)
             );
             CREATE TABLE IF NOT EXISTS outbox(
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 session_id TEXT NOT NULL,
                 step_id INTEGER NOT NULL,
                 payload TEXT NOT NULL,
                 created_at INTEGER NOT NULL
             );
             CREATE UNIQUE INDEX IF NOT EXISTS outbox_step_once
                 ON outbox(session_id, step_id);
             -- Runda 10: granica tury. `base_step` = pierwszy `step_id` tej tury,
             -- `running` = tura jeszcze sie nie zamknela (crash albo trwa).
             CREATE TABLE IF NOT EXISTS turns(
                 session_id TEXT NOT NULL,
                 turn_no INTEGER NOT NULL,
                 status TEXT NOT NULL,
                 base_step INTEGER NOT NULL,
                 started_at INTEGER NOT NULL,
                 finished_at INTEGER,
                 PRIMARY KEY(session_id, turn_no)
             );
             -- Runda 11 (M1): zadanie jako wiersz (`tasks`), nie tylko kolumna w
             -- `sessions`. Jedna sesja = jedno zadanie, wiec `id` = `session_id`.
             CREATE TABLE IF NOT EXISTS tasks(
                 id TEXT PRIMARY KEY,
                 session_id TEXT NOT NULL,
                 text TEXT NOT NULL,
                 status TEXT NOT NULL,
                 created_at INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS tasks_session ON tasks(session_id);
             -- Runda 11 (M1): kopia kazdej linii JSONL. `step` z `data.step`,
             -- `payload` = cala linia eventu (ten sam `kind`, ta sama tresc).
             CREATE TABLE IF NOT EXISTS transcripts(
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 session_id TEXT NOT NULL,
                 step INTEGER,
                 kind TEXT NOT NULL,
                 payload TEXT NOT NULL,
                 at INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS transcripts_session_step
                 ON transcripts(session_id, step);
             -- FTS5 po `payload`; tabela zewnetrznej tresci, wiec kopii nie ma.
             CREATE VIRTUAL TABLE IF NOT EXISTS transcripts_fts USING fts5(
                 payload,
                 content='transcripts',
                 content_rowid='id'
             );
             CREATE TRIGGER IF NOT EXISTS transcripts_ai AFTER INSERT ON transcripts BEGIN
                 INSERT INTO transcripts_fts(rowid, payload) VALUES (new.id, new.payload);
             END;
             CREATE TRIGGER IF NOT EXISTS transcripts_ad AFTER DELETE ON transcripts BEGIN
                 INSERT INTO transcripts_fts(transcripts_fts, rowid, payload)
                     VALUES('delete', old.id, old.payload);
             END;
             CREATE TRIGGER IF NOT EXISTS transcripts_au AFTER UPDATE ON transcripts BEGIN
                 INSERT INTO transcripts_fts(transcripts_fts, rowid, payload)
                     VALUES('delete', old.id, old.payload);
                 INSERT INTO transcripts_fts(rowid, payload) VALUES (new.id, new.payload);
             END;",
        )
        .map_err(|e| format!("sqlite schema: {}", e))?;
        // Backfill tylko przy pierwszym otwarciu bazy z FTS (stara baza: tabela
        // `transcripts` jest nowa i pusta, wiec przebudowa jest darmowa).
        let version: i64 = c
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap_or(0);
        if version < 1 {
            c.execute_batch(
                "INSERT INTO transcripts_fts(transcripts_fts) VALUES('rebuild');
                 PRAGMA user_version = 1;",
            )
            .map_err(|e| format!("sqlite fts rebuild: {}", e))?;
        }
        Ok(())
    }

    pub fn create_session(&self, session_id: &str, task: &str) -> Result<(), String> {
        let c = self.conn()?;
        c.execute(
            "INSERT OR IGNORE INTO sessions(id, task, created_at) VALUES(?1, ?2, ?3)",
            params![session_id, task, now_ts()],
        )
        .map_err(|e| format!("sqlite session: {}", e))?;
        Ok(())
    }

    fn max_step_on(c: &Connection, session_id: &str) -> Result<u64, String> {
        c.query_row(
            "SELECT COALESCE(MAX(step_id), 0) FROM steps WHERE session_id=?1",
            params![session_id],
            |row| row.get::<_, i64>(0),
        )
        .map(|v| v.max(0) as u64)
        .map_err(|e| format!("sqlite max_step: {}", e))
    }

    /// Otwiera ture sesji i zwraca jej granice.
    ///
    /// Runda 10: numeracja jest ciagla w obrebie sesji. Nowa tura startuje od
    /// `max(step_id)+1`, wiec identyfikatory z historii nigdy sie nie powtarzaja
    /// (brak kolizji `step_id` miedzy turami). Tura, ktora zostala otwarta
    /// i nie zamknieta (`running`), jest wznawiana z tym samym `base_step` —
    /// tylko w jej obrebie dziala replay i regula "ten sam `step_id` z innymi
    /// argumentami".
    pub fn open_turn(&self, session_id: &str) -> Result<TurnRow, String> {
        let c = self.conn()?;
        let last = {
            let mut stmt = c
                .prepare(
                    "SELECT turn_no, status, base_step FROM turns
                     WHERE session_id=?1 ORDER BY turn_no DESC LIMIT 1",
                )
                .map_err(|e| format!("sqlite turn: {}", e))?;
            let mut rows = stmt
                .query(params![session_id])
                .map_err(|e| format!("sqlite turn: {}", e))?;
            match rows.next().map_err(|e| format!("sqlite turn: {}", e))? {
                Some(row) => Some(TurnRow {
                    turn_no: row.get(0).map_err(|e| format!("sqlite turn: {}", e))?,
                    status: row.get(1).map_err(|e| format!("sqlite turn: {}", e))?,
                    base_step: row
                        .get::<_, i64>(2)
                        .map_err(|e| format!("sqlite turn: {}", e))? as u64,
                }),
                None => None,
            }
        };

        let turn = match last {
            // Crash w obrebie tej tury: kontynuujemy ja, nie zaczynamy nowej.
            Some(row) if row.status == "running" => row,
            // Tura zamknieta (albo sesja sprzed rundy 10): nowa tura od max+1.
            previous => {
                let turn_no = previous.map(|p| p.turn_no).unwrap_or(0) + 1;
                let base_step = Store::max_step_on(&c, session_id)? + 1;
                c.execute(
                    "INSERT INTO turns(session_id, turn_no, status, base_step, started_at)
                     VALUES(?1, ?2, 'running', ?3, ?4)",
                    params![session_id, turn_no, base_step as i64, now_ts()],
                )
                .map_err(|e| format!("sqlite turn: {}", e))?;
                TurnRow {
                    turn_no,
                    status: "running".to_string(),
                    base_step,
                }
            }
        };
        Ok(turn)
    }

    /// Zamkniecie tury po zakonczeniu przebiegu. Od tej chwili jej kroki sa
    /// historia: replay i regula `step_id` dzialaja tylko w nastepnej turze.
    pub fn close_turn(&self, session_id: &str, turn_no: i64) -> Result<(), String> {
        let c = self.conn()?;
        let changed = c
            .execute(
                "UPDATE turns SET status='done', finished_at=?3
                 WHERE session_id=?1 AND turn_no=?2 AND status='running'",
                params![session_id, turn_no, now_ts()],
            )
            .map_err(|e| format!("sqlite turn close: {}", e))?;
        if changed == 0 {
            return Err(format!(
                "sqlite turn close: tura {} nie byla otwarta",
                turn_no
            ));
        }
        Ok(())
    }

    /// Wszystkie tury sesji w kolejnosci (diagnostyka, testy).
    #[allow(dead_code)]
    pub fn turns_for_session(&self, session_id: &str) -> Result<Vec<TurnRow>, String> {
        let c = self.conn()?;
        let mut stmt = c
            .prepare(
                "SELECT turn_no, status, base_step FROM turns
                 WHERE session_id=?1 ORDER BY turn_no",
            )
            .map_err(|e| format!("sqlite turns: {}", e))?;
        let rows = stmt
            .query_map(params![session_id], |row| {
                Ok(TurnRow {
                    turn_no: row.get(0)?,
                    status: row.get(1)?,
                    base_step: row.get::<_, i64>(2)? as u64,
                })
            })
            .map_err(|e| format!("sqlite turns: {}", e))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| format!("sqlite turns: {}", e))?);
        }
        Ok(out)
    }

    /// Przed wykonaniem toola: wiersz `proposed`. Jesli wiersz juz `done`, nie nadpisuje.
    pub fn proposed(
        &self,
        session_id: &str,
        step_id: u64,
        tool: &str,
        args_json: &str,
    ) -> Result<(), String> {
        let c = self.conn()?;
        c.execute(
            "INSERT OR IGNORE INTO steps(session_id, step_id, tool, args_json, status, created_at)
             VALUES(?1, ?2, ?3, ?4, 'proposed', ?5)",
            params![session_id, step_id as i64, tool, args_json, now_ts()],
        )
        .map_err(|e| format!("sqlite proposed: {}", e))?;
        Ok(())
    }

    /// Pelny wiersz kroku — do decyzji replay/skip/execute.
    pub fn fetch_step(&self, session_id: &str, step_id: u64) -> Result<Option<StepRow>, String> {
        let c = self.conn()?;
        let mut stmt = c
            .prepare(
                "SELECT step_id, tool, args_json, status, result_json FROM steps
                 WHERE session_id=?1 AND step_id=?2",
            )
            .map_err(|e| format!("sqlite fetch: {}", e))?;
        let mut rows = stmt
            .query(params![session_id, step_id as i64])
            .map_err(|e| format!("sqlite fetch: {}", e))?;
        match rows.next().map_err(|e| format!("sqlite fetch: {}", e))? {
            Some(row) => Ok(Some(StepRow {
                step_id: row.get(0).map_err(|e| format!("sqlite fetch: {}", e))?,
                tool: row.get(1).map_err(|e| format!("sqlite fetch: {}", e))?,
                args_json: row.get(2).map_err(|e| format!("sqlite fetch: {}", e))?,
                status: row.get(3).map_err(|e| format!("sqlite fetch: {}", e))?,
                result_json: row.get(4).map_err(|e| format!("sqlite fetch: {}", e))?,
            })),
            None => Ok(None),
        }
    }

    /// Wszystkie kroki sesji w kolejnosci `step_id`.
    #[allow(dead_code)] // API diagnostyczne (testy, przyszly `status`)
    pub fn steps_for_session(&self, session_id: &str) -> Result<Vec<StepRow>, String> {
        let c = self.conn()?;
        let mut stmt = c
            .prepare(
                "SELECT step_id, tool, args_json, status, result_json FROM steps
                 WHERE session_id=?1 ORDER BY step_id",
            )
            .map_err(|e| format!("sqlite steps: {}", e))?;
        let rows = stmt
            .query_map(params![session_id], |row| {
                Ok(StepRow {
                    step_id: row.get(0)?,
                    tool: row.get(1)?,
                    args_json: row.get(2)?,
                    status: row.get(3)?,
                    result_json: row.get(4)?,
                })
            })
            .map_err(|e| format!("sqlite steps: {}", e))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| format!("sqlite steps: {}", e))?);
        }
        Ok(out)
    }

    /// Krok, ktory byl `proposed` w chwili crashu, jest zamykany jako `abandoned`:
    /// at-most-once — efekt mogl juz czesciowo zajsc, wiec nie powtarzamy.
    pub fn mark_abandoned(
        &self,
        session_id: &str,
        step_id: u64,
        note_json: &str,
    ) -> Result<(), String> {
        let c = self.conn()?;
        let changed = c
            .execute(
                "UPDATE steps SET status='abandoned', result_json=?3
                 WHERE session_id=?1 AND step_id=?2 AND status='proposed'",
                params![session_id, step_id as i64, note_json],
            )
            .map_err(|e| format!("sqlite abandoned: {}", e))?;
        if changed == 0 {
            return Err(format!(
                "sqlite abandoned: krok {} nie byl w stanie proposed",
                step_id
            ));
        }
        Ok(())
    }

    /// Po wykonaniu: `done` + wynik. Zwykly update (bez outbox).
    pub fn complete(
        &self,
        session_id: &str,
        step_id: u64,
        result_json: &str,
    ) -> Result<(), String> {
        let c = self.conn()?;
        let changed = c
            .execute(
                "UPDATE steps SET status='done', result_json=?3 WHERE session_id=?1 AND step_id=?2",
                params![session_id, step_id as i64, result_json],
            )
            .map_err(|e| format!("sqlite complete: {}", e))?;
        if changed == 0 {
            return Err(format!(
                "sqlite complete: krok {} nie istnieje w tabeli steps",
                step_id
            ));
        }
        Ok(())
    }

    /// `notify.send`: zmiana statusu kroku + rekord w outbox w TEJ SAMEJ transakcji.
    pub fn complete_with_outbox(
        &self,
        session_id: &str,
        step_id: u64,
        result_json: &str,
        outbox_payload: &str,
    ) -> Result<(), String> {
        let mut c = self.conn()?;
        let tx = c
            .transaction()
            .map_err(|e| format!("sqlite tx: {}", e))?;
        let changed = tx
            .execute(
                "UPDATE steps SET status='done', result_json=?3 WHERE session_id=?1 AND step_id=?2",
                params![session_id, step_id as i64, result_json],
            )
            .map_err(|e| format!("sqlite complete: {}", e))?;
        if changed == 0 {
            return Err(format!(
                "sqlite complete: krok {} nie istnieje w tabeli steps",
                step_id
            ));
        }
        // Idempotentne: powtorka tej samej transakcji nie dubluje efektu.
        tx.execute(
            "INSERT OR IGNORE INTO outbox(session_id, step_id, payload, created_at)
             VALUES(?1, ?2, ?3, ?4)",
            params![session_id, step_id as i64, outbox_payload, now_ts()],
        )
        .map_err(|e| format!("sqlite outbox: {}", e))?;
        tx.commit().map_err(|e| format!("sqlite commit: {}", e))?;
        Ok(())
    }

    #[allow(dead_code)] // API diagnostyczne (testy)
    pub fn outbox_for_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<String>, String> {        let c = self.conn()?;
        let mut stmt = c
            .prepare("SELECT payload FROM outbox WHERE session_id=?1 ORDER BY id")
            .map_err(|e| format!("sqlite outbox read: {}", e))?;
        let rows = stmt
            .query_map(params![session_id], |row| row.get::<_, String>(0))
            .map_err(|e| format!("sqlite outbox read: {}", e))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| format!("sqlite outbox read: {}", e))?);
        }
        Ok(out)
    }

    // --- Runda 11 (M1): zadania -------------------------------------------------

    /// Wiersz zadania powstaje przy starcie przebiegu. `INSERT OR IGNORE`, bo
    /// `--resume` wchodzi w te sama sesje i nie moze nadpisac pierwotnego tekstu.
    pub fn start_task(&self, id: &str, session_id: &str, text: &str) -> Result<(), String> {
        let c = self.conn()?;
        c.execute(
            "INSERT OR IGNORE INTO tasks(id, session_id, text, status, created_at)
             VALUES(?1, ?2, ?3, 'running', ?4)",
            params![id, session_id, text, now_ts()],
        )
        .map_err(|e| format!("sqlite task start: {}", e))?;
        Ok(())
    }

    /// Status koncowy zadania — te same slowa co w podsumowaniu sesji.
    pub fn set_task_status(&self, id: &str, status: &str) -> Result<(), String> {
        let c = self.conn()?;
        let changed = c
            .execute(
                "UPDATE tasks SET status=?2 WHERE id=?1",
                params![id, status],
            )
            .map_err(|e| format!("sqlite task status: {}", e))?;
        if changed == 0 {
            return Err(format!("sqlite task status: zadanie {} nie istnieje", id));
        }
        Ok(())
    }

    #[allow(dead_code)] // API diagnostyczne (testy, przyszly `status`)
    pub fn task(&self, id: &str) -> Result<Option<TaskRow>, String> {
        let c = self.conn()?;
        let mut stmt = c
            .prepare("SELECT id, session_id, text, status FROM tasks WHERE id=?1")
            .map_err(|e| format!("sqlite task: {}", e))?;
        let mut rows = stmt
            .query(params![id])
            .map_err(|e| format!("sqlite task: {}", e))?;
        match rows.next().map_err(|e| format!("sqlite task: {}", e))? {
            Some(row) => Ok(Some(TaskRow {
                id: row.get(0).map_err(|e| format!("sqlite task: {}", e))?,
                session_id: row.get(1).map_err(|e| format!("sqlite task: {}", e))?,
                text: row.get(2).map_err(|e| format!("sqlite task: {}", e))?,
                status: row.get(3).map_err(|e| format!("sqlite task: {}", e))?,
            })),
            None => Ok(None),
        }
    }

    // --- Runda 11 (M1): transkrypty i FTS --------------------------------------

    /// Dopisuje jedna linie JSONL do `transcripts`. Wolane z `Session::event`,
    /// wiec inicjuje schemat sam, gdyby sesja pisala przed `init()`.
    pub fn append_transcript(
        &self,
        session_id: &str,
        step: Option<u32>,
        kind: &str,
        payload: &str,
    ) -> Result<(), String> {
        match self.insert_transcript(session_id, step, kind, payload) {
            Err(e) if e.contains("no such table") => {
                self.init()?;
                self.insert_transcript(session_id, step, kind, payload)
            }
            other => other,
        }
    }

    fn insert_transcript(
        &self,
        session_id: &str,
        step: Option<u32>,
        kind: &str,
        payload: &str,
    ) -> Result<(), String> {
        let c = self.conn()?;
        c.execute(
            "INSERT INTO transcripts(session_id, step, kind, payload, at)
             VALUES(?1, ?2, ?3, ?4, ?5)",
            params![session_id, step.map(|s| s as i64), kind, payload, now_ts()],
        )
        .map_err(|e| format!("sqlite transcript: {}", e))?;
        Ok(())
    }

    /// Wiersze transkryptu sesji w kolejnosci zapisu (dowod: JSONL vs baza).
    #[allow(dead_code)] // API diagnostyczne (testy, pozniejszy `status`)
    pub fn transcripts_for_session(&self, session_id: &str) -> Result<Vec<TranscriptRow>, String> {
        let c = self.conn()?;
        let mut stmt = c
            .prepare(
                "SELECT id, session_id, step, kind, payload, at FROM transcripts
                 WHERE session_id=?1 ORDER BY id",
            )
            .map_err(|e| format!("sqlite transcripts: {}", e))?;
        let rows = stmt
            .query_map(params![session_id], |row| {
                Ok(TranscriptRow {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    step: row.get(2)?,
                    kind: row.get(3)?,
                    payload: row.get(4)?,
                    at: row.get(5)?,
                })
            })
            .map_err(|e| format!("sqlite transcripts: {}", e))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| format!("sqlite transcripts: {}", e))?);
        }
        Ok(out)
    }

    /// `search_transcripts(query)`: frazy z zadania po `payload`.
    ///
    /// Puste zapytanie (albo samo `::`, `;`, wielokropek) zwraca pusta liste bez
    /// dotykania `MATCH` — to nie jest blad. Kazdy token idzie w cudzyslowie i
    /// jest OR-owany, wiec znaki specjalne FTS nie psuja zapytania.
    pub fn search_transcripts(&self, query: &str) -> Result<Vec<TranscriptHit>, String> {
        let Some(expr) = fts_query(query) else {
            return Ok(Vec::new());
        };
        let c = self.conn()?;
        let mut stmt = c
            .prepare(
                "SELECT t.session_id, COALESCE(t.step, 0),
                        snippet(transcripts_fts, 0, '[', ']', '…', 12)
                 FROM transcripts_fts
                 JOIN transcripts t ON t.id = transcripts_fts.rowid
                 WHERE transcripts_fts MATCH ?1
                 ORDER BY bm25(transcripts_fts)
                 LIMIT ?2",
            )
            .map_err(|e| format!("sqlite search: {}", e))?;
        let rows = stmt
            .query_map(params![expr, HISTORY_HITS as i64], |row| {
                Ok(TranscriptHit {
                    session_id: row.get(0)?,
                    step: row.get(1)?,
                    snippet: row.get(2)?,
                })
            })
            .map_err(|e| format!("sqlite search: {}", e))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| format!("sqlite search: {}", e))?);
        }
        Ok(out)
    }
}

/// Wyrazenie FTS5 z tekstu zadania: tokeny alfanumeryczne w cudzyslowie,
/// polaczone `OR`. `None` = nie ma czego szukac (puste zapytanie).
fn fts_query(query: &str) -> Option<String> {
    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    for ch in query.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            current.push(ch);
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    if tokens.is_empty() {
        return None;
    }
    Some(
        tokens
            .iter()
            .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" OR "),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "arndtai-store-{}-{}-{}",
            tag,
            std::process::id(),
            now_ts()
        ));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    /// Faza B pkt 8 / F21: krok po restarcie jest czytany jako `done` z wynikiem,
    /// wiec petla moze pominac wykonanie (test konczy sie na decyzji, nie na liczniku).
    #[test]
    fn done_step_survives_restart_with_result() {
        let dir = test_dir("resume");
        let store = Store::new(&dir.join("state.db"));
        store.init().unwrap();
        store.create_session("s1", "zadanie").unwrap();
        store.proposed("s1", 1, "fs.write", "{\"path\":\"a.txt\"}").unwrap();
        store.complete("s1", 1, "{\"ok\":true}").unwrap();
        drop(store);

        let after_restart = Store::new(&dir.join("state.db"));
        after_restart.init().unwrap();
        let row = after_restart.fetch_step("s1", 1).unwrap().expect("krok 1");
        assert_eq!(row.status, "done");
        assert_eq!(row.tool, "fs.write");
        assert_eq!(row.result_json.as_deref(), Some("{\"ok\":true}"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wal_and_busy_timeout() {
        let dir = test_dir("wal");
        let store = Store::for_workspace(&dir);
        store.init().unwrap();
        assert_eq!(store.journal_mode().unwrap().to_lowercase(), "wal");
        assert_eq!(store.busy_timeout().unwrap(), 5000);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn outbox_is_once_per_step() {
        let dir = test_dir("outbox-once");
        let store = Store::for_workspace(&dir);
        store.init().unwrap();
        store.create_session("s1", "t").unwrap();
        store.proposed("s1", 1, "notify.send", "{\"message\":\"a\"}").unwrap();
        store
            .complete_with_outbox("s1", 1, "{\"ok\":true}", "{\"message\":\"a\"}")
            .unwrap();
        store
            .complete_with_outbox("s1", 1, "{\"ok\":true}", "{\"message\":\"a\"}")
            .unwrap();
        assert_eq!(store.outbox_for_session("s1").unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn complete_on_missing_step_is_an_error() {
        let dir = test_dir("complete-missing");
        let store = Store::for_workspace(&dir);
        store.init().unwrap();
        store.create_session("s1", "t").unwrap();
        assert!(store.complete("s1", 7, "{}").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Runda 10: po zamknietej turze nowa tura startuje od `max(step_id)+1`,
    /// a tura przerwana crashem (wciaz `running`) jest kontynuowana od swojego
    /// pierwszego kroku — bez przeskoku numeracji.
    #[test]
    fn turn_boundary_follows_history_and_resumes_a_crash() {
        let dir = test_dir("turns");
        let store = Store::for_workspace(&dir);
        store.init().unwrap();
        store.create_session("s1", "t").unwrap();

        let first = store.open_turn("s1").unwrap();
        assert_eq!((first.turn_no, first.base_step), (1, 1));
        for step in 1..=3u64 {
            store.proposed("s1", step, "fs.write", "{}").unwrap();
            store.complete("s1", step, "{\"ok\":true}").unwrap();
        }
        store.close_turn("s1", first.turn_no).unwrap();

        let second = store.open_turn("s1").unwrap();
        assert_eq!(second.turn_no, 2);
        assert_eq!(second.base_step, 4, "pierwszy krok nowej tury = max+1");

        // Crash w tej turze: krok `proposed` bez wyniku, tura zostaje `running`.
        store.proposed("s1", 4, "fs.append", "{}").unwrap();
        let resumed = store.open_turn("s1").unwrap();
        assert_eq!(resumed.turn_no, 2, "crash = kontynuacja tej samej tury");
        assert_eq!(resumed.base_step, 4, "ten sam poczatek tury");
        let turns = store.turns_for_session("s1").unwrap();
        assert_eq!(turns.len(), 2, "bez nowej tury po crashu");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Runda 11 (M1): baza sprzed migracji otwiera sie i dziala, a po `init`
    /// ma `tasks` + `transcripts`. Stare sesje i kroki zostaja czytelne.
    #[test]
    fn old_database_opens_and_gains_tasks_and_transcripts() {
        let dir = test_dir("migracja");
        let db = dir.join("state.db");
        {
            let c = Connection::open(&db).unwrap();
            c.execute_batch(
                "CREATE TABLE sessions(
                     id TEXT PRIMARY KEY,
                     task TEXT NOT NULL,
                     created_at INTEGER NOT NULL
                 );
                 CREATE TABLE steps(
                     session_id TEXT NOT NULL,
                     step_id INTEGER NOT NULL,
                     tool TEXT NOT NULL,
                     args_json TEXT NOT NULL,
                     status TEXT NOT NULL,
                     result_json TEXT,
                     created_at INTEGER NOT NULL,
                     PRIMARY KEY(session_id, step_id)
                 );
                 INSERT INTO sessions VALUES('stara', 'stare zadanie', 10);
                 INSERT INTO steps VALUES('stara', 1, 'fs.write', '{}', 'done', '{\"ok\":true}', 11);",
            )
            .unwrap();
        }

        let store = Store::new(&db);
        store.init().unwrap();

        let c = Connection::open(&db).unwrap();
        let has_table = |name: &str| -> bool {
            c.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                params![name],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
                == 1
        };
        assert!(has_table("tasks"), "nowa tabela zadan");
        assert!(has_table("transcripts"), "nowa tabela transkryptow");

        // Stare dane bez zmian.
        let task: String = c
            .query_row("SELECT task FROM sessions WHERE id='stara'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(task, "stare zadanie");
        assert_eq!(store.fetch_step("stara", 1).unwrap().unwrap().status, "done");
        assert_eq!(store.steps_for_session("stara").unwrap().len(), 1);

        // Druga migracja jest idempotentna.
        store.init().unwrap();
        store.init().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Runda 11 (M1): zadanie ma tekst z chwili startu i status koncowy.
    #[test]
    fn task_row_keeps_text_and_final_status() {
        let dir = test_dir("task");
        let store = Store::for_workspace(&dir);
        store.init().unwrap();

        store.start_task("s1", "s1", "write a.txt :: tresc").unwrap();
        assert_eq!(store.task("s1").unwrap().unwrap().status, "running");

        // `--resume` nie podmienia tekstu zadania.
        store.start_task("s1", "s1", "inne zadanie").unwrap();
        assert_eq!(store.task("s1").unwrap().unwrap().text, "write a.txt :: tresc");

        store.set_task_status("s1", "done").unwrap();
        let row = store.task("s1").unwrap().unwrap();
        assert_eq!(row.text, "write a.txt :: tresc");
        assert_eq!(row.status, "done");
        assert_eq!(row.session_id, "s1");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Runda 11 (M1): kazda linia JSONL ma wiersz w `transcripts` (ta sama
    /// kolejnosc, ten sam `kind`, ten sam `step`).
    #[test]
    fn transcripts_mirror_every_jsonl_event() {
        use crate::context::Session;
        use serde_json::json;

        let dir = test_dir("transcripts");
        let store = Store::for_workspace(&dir);
        store.init().unwrap();

        let session = Session::start(&dir, "write a.txt :: alfa");
        session.event("thought", json!({ "step": 1, "thought": "pisze" }));
        session.event(
            "tool_call",
            json!({ "step": 1, "tool": "fs.write", "args": { "path": "a.txt" } }),
        );
        session.event(
            "tool_result",
            json!({ "step": 1, "tool": "fs.write", "ok": true, "bytes": 4 }),
        );
        session.event(
            "summary",
            json!({ "status": "DONE", "steps": 1, "tool_calls": 1, "denied": 0, "reason": "gotowe" }),
        );

        let lines = std::fs::read_to_string(&session.file)
            .unwrap()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count();
        let rows = store.transcripts_for_session(&session.id).unwrap();
        assert_eq!(rows.len(), lines, "wiersz na kazda linie JSONL");
        assert_eq!(rows[0].kind, "task");
        assert_eq!(rows[0].step, None);
        assert_eq!(rows[1].kind, "thought");
        assert_eq!(rows[1].step, Some(1));
        assert_eq!(rows.last().unwrap().kind, "summary");
        assert_eq!(rows.last().unwrap().step, None);
        assert!(
            rows[0].payload.contains("\"kind\":\"task\""),
            "payload to ta sama linia eventu: {}",
            rows[0].payload
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Runda 11 (M1): FTS trafia po tresci, przestaje trafiac po zmianie
    /// tresci i po usunieciu wiersza.
    #[test]
    fn fts_follows_insert_update_and_delete() {
        let dir = test_dir("fts");
        let store = Store::for_workspace(&dir);
        store.init().unwrap();
        store
            .append_transcript(
                "s1",
                Some(1),
                "tool_result",
                "{\"kind\":\"tool_result\",\"data\":{\"tool\":\"fs.read\",\"output\":\"raport kwartalny\"}}",
            )
            .unwrap();

        let hits = store.search_transcripts("raport").unwrap();
        assert_eq!(hits.len(), 1, "trafienie po tresci kroku");
        assert_eq!(hits[0].session_id, "s1");
        assert_eq!(hits[0].step, 1);
        assert!(hits[0].snippet.contains("raport"), "{}", hits[0].snippet);

        let c = Connection::open(&store.path).unwrap();
        c.execute(
            "UPDATE transcripts SET payload=?1 WHERE session_id='s1'",
            params!["{\"kind\":\"tool_result\",\"data\":{\"output\":\"notatka o budzecie\"}}"],
        )
        .unwrap();
        assert!(
            store.search_transcripts("raport").unwrap().is_empty(),
            "stary fragment nie trafia"
        );
        assert_eq!(store.search_transcripts("budzecie").unwrap().len(), 1);

        c.execute("DELETE FROM transcripts WHERE session_id='s1'", [])
            .unwrap();
        assert!(
            store.search_transcripts("budzecie").unwrap().is_empty(),
            "usuniety wiersz nie trafia"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// M1, poz. 2 z `docs/STATUS.md`: baza zapisana przed wprowadzeniem FTS nie
    /// ma indeksu ani `user_version`, wiec po migracji stare transkrypty musza
    /// byc przeszukiwalne — inaczej `pro` widzi tylko sesje po migracji.
    #[test]
    fn backfill_indexes_transcripts_written_before_fts_existed() {
        let dir = test_dir("fts-backfill");
        let path = dir.join(".agent").join("state.db");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // Stara baza: tabela `transcripts` bez FTS, bez triggerow, bez wersji.
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch(
                "CREATE TABLE transcripts(
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     session_id TEXT NOT NULL,
                     step INTEGER,
                     kind TEXT NOT NULL,
                     payload TEXT NOT NULL,
                     at INTEGER NOT NULL
                 );
                 INSERT INTO transcripts(session_id, step, kind, payload, at)
                     VALUES('old-session', 3, 'tool_result',
                            '{\"kind\":\"tool_result\",\"data\":{\"output\":\"raport kwartalny\"}}', 0);",
            )
            .unwrap();
            let version: i64 = c
                .query_row("PRAGMA user_version", [], |row| row.get(0))
                .unwrap();
            assert_eq!(version, 0, "stara baza nie ma wersji schematu");
        }

        let store = Store::for_workspace(&dir);
        store.init().unwrap();

        let hits = store.search_transcripts("kwartalny").unwrap();
        assert_eq!(hits.len(), 1, "stary transkrypt trafia po migracji");
        assert_eq!(hits[0].session_id, "old-session");
        assert_eq!(hits[0].step, 3);

        // Migracja jest jednorazowa: drugie otwarcie nic nie gubi.
        drop(store);
        let reopened = Store::for_workspace(&dir);
        reopened.init().unwrap();
        assert_eq!(reopened.search_transcripts("kwartalny").unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Runda 11 (M1): puste zapytanie to pusta lista, nie blad.
    #[test]
    fn empty_search_is_not_an_error() {
        let dir = test_dir("fts-empty");
        let store = Store::for_workspace(&dir);
        store.init().unwrap();
        assert!(store.search_transcripts("").unwrap().is_empty());
        assert!(store.search_transcripts("   ").unwrap().is_empty());
        assert!(store.search_transcripts(":: ; …").unwrap().is_empty());
        assert!(store
            .search_transcripts("fraza ktorej nie ma")
            .unwrap()
            .is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

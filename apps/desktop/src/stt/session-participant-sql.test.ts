import { describe, expect, it } from "vitest";

import { SESSION_PARTICIPANT_HUMAN_IDS_SQL } from "~/stt/session-participant-sql";

// node:sqlite is reached through process.getBuiltinModule rather than an
// import: this suite runs under the browser-flavoured default environment,
// whose bundler refuses to resolve Node built-ins. Vitest itself runs in
// Node, so the module is there.
type SqliteDatabase = {
  exec: (sql: string) => void;
  prepare: (sql: string) => {
    run: (...params: unknown[]) => unknown;
    all: (...params: unknown[]) => Array<Record<string, unknown>>;
  };
  close: () => void;
};

const { DatabaseSync } = process.getBuiltinModule("node:sqlite") as {
  DatabaseSync: new (path: string) => SqliteDatabase;
};

// The guard that keeps the current user out of their own participant list is
// pure SQL, so mocked rows cannot prove it. These cases run the shipped
// statement against a real SQLite and rebuild the shapes measured on the operator's own installation
// database on 2026-09-02: an own contact carrying no email at all, and the
// calendar copy of him invited under a work address.
describe("session participant guard against real SQLite", () => {
  function participantHumanIds(
    seed: (database: SqliteDatabase) => void,
    sessionId = "session-1",
  ): string[] {
    const database = new DatabaseSync(":memory:");
    database.exec(`
      CREATE TABLE humans (id TEXT, email TEXT DEFAULT '', deleted_at TEXT);
      CREATE TABLE calendars (id TEXT, source TEXT DEFAULT '', deleted_at TEXT);
      CREATE TABLE events (
        id TEXT, calendar_id TEXT DEFAULT '',
        participants_json TEXT DEFAULT '[]', deleted_at TEXT
      );
      CREATE TABLE sessions (id TEXT, owner_user_id TEXT DEFAULT '', event_id TEXT DEFAULT '');
      CREATE TABLE session_participants (
        session_id TEXT, human_id TEXT, email TEXT DEFAULT '',
        source TEXT DEFAULT 'auto', deleted_at TEXT
      );
    `);
    seed(database);
    const rows = database
      .prepare(SESSION_PARTICIPANT_HUMAN_IDS_SQL)
      .all(sessionId) as Array<{ human_id: string }>;
    database.close();
    return rows.map((row) => row.human_id);
  }

  function seedGemesseneForm(
    database: SqliteDatabase,
    {
      selfInviteEmail = "Mads.Verlin@nordwerk.example",
      isCurrentUser = true,
    } = {},
  ) {
    database.exec(`
      INSERT INTO humans (id, email) VALUES
        ('mads', ''),
        ('mads-calendar', 'mads.verlin@nordwerk.example'),
        ('tom', 'tom.kestroll@nordwerk.example');
      INSERT INTO sessions (id, owner_user_id, event_id)
        VALUES ('session-1', 'mads', 'event-1');
      INSERT INTO session_participants (session_id, human_id, email) VALUES
        ('session-1', 'tom', 'tom.kestroll@nordwerk.example'),
        ('session-1', 'mads-calendar', 'mads.verlin@nordwerk.example');
    `);
    database.exec(
      "INSERT INTO calendars (id, source) VALUES ('calendar-1', 'iCloud')",
    );
    database
      .prepare(
        "INSERT INTO events (id, calendar_id, participants_json) VALUES ('event-1', 'calendar-1', ?)",
      )
      .run(
        JSON.stringify([
          {
            name: "Tom Kestroll | nordwerk",
            email: "tom.kestroll@nordwerk.example",
            is_organizer: true,
            is_current_user: false,
          },
          {
            name: "Mads Verlin | nordwerk",
            email: selfInviteEmail,
            is_organizer: false,
            is_current_user: isCurrentUser,
          },
        ]),
      );
  }

  it("drops the calendar copy of the owner even when the owner has no email", () => {
    expect(participantHumanIds(seedGemesseneForm)).toEqual(["tom"]);
  });

  it("drops a calendar copy invited under a second address", () => {
    expect(
      participantHumanIds((database) => {
        seedGemesseneForm(database);
        database.exec(`
          INSERT INTO humans (id, email) VALUES ('mads-private', 'mads@nordwerk.example');
          INSERT INTO session_participants (session_id, human_id, email)
            VALUES ('session-1', 'mads-private', 'mads@nordwerk.example');
          UPDATE events SET participants_json = json_insert(
            participants_json, '$[#]',
            json_object('email', 'mads@nordwerk.example', 'is_current_user', json('true'))
          ) WHERE id = 'event-1';
        `);
      }),
    ).toEqual(["tom"]);
  });

  it("keeps every remote when the calendar flags nobody as the current user", () => {
    expect(
      participantHumanIds((database) =>
        seedGemesseneForm(database, { isCurrentUser: false }),
      ),
    ).toEqual(["mads-calendar", "tom"]);
  });

  it("keeps every remote when the flag is a string instead of a boolean", () => {
    const ids = participantHumanIds((database) => {
      seedGemesseneForm(database, { isCurrentUser: false });
      database.exec(`
        UPDATE events SET participants_json = replace(
          participants_json, '"is_current_user":false', '"is_current_user":"true"'
        ) WHERE id = 'event-1';
      `);
    });
    expect(ids).toEqual(["mads-calendar", "tom"]);
  });

  it("still lists remotes when the event payload is not valid JSON", () => {
    expect(
      participantHumanIds((database) => {
        seedGemesseneForm(database);
        database.exec(
          "UPDATE events SET participants_json = 'kaputt' WHERE id = 'event-1'",
        );
      }),
    ).toEqual(["mads-calendar", "tom"]);
  });

  it("keeps the owner's own email guard working without any calendar event", () => {
    expect(
      participantHumanIds((database) => {
        database.exec(`
          INSERT INTO humans (id, email) VALUES
            ('mads', 'mads.verlin@nordwerk.example'),
            ('mads-calendar', 'Mads.Verlin@nordwerk.example'),
            ('tom', 'tom.kestroll@nordwerk.example');
          INSERT INTO sessions (id, owner_user_id) VALUES ('session-1', 'mads');
          INSERT INTO session_participants (session_id, human_id, email) VALUES
            ('session-1', 'tom', 'tom.kestroll@nordwerk.example'),
            ('session-1', 'mads-calendar', 'Mads.Verlin@nordwerk.example'),
            ('session-1', 'mads', 'mads.verlin@nordwerk.example');
        `);
      }),
    ).toEqual(["tom"]);
  });

  it("leaves excluded and deleted rows out, and never guesses by name", () => {
    expect(
      participantHumanIds((database) => {
        seedGemesseneForm(database);
        database.exec(`
          INSERT INTO humans (id, email) VALUES
            ('vilken', 'ilka.roden@nordwerk.example'),
            ('namensvetter', '');
          INSERT INTO session_participants (session_id, human_id, email, source, deleted_at) VALUES
            ('session-1', 'vilken', 'ilka.roden@nordwerk.example', 'excluded', NULL),
            ('session-1', 'namensvetter', '', 'auto', NULL);
        `);
      }),
    ).toEqual(["namensvetter", "tom"]);
  });
  // The destructive case: the provider puts the flag on the wrong attendee.
  // The sync then drops the real remote and cleans up his row as stale, and
  // the user's leftover copy is all that is left -- close to being stamped
  // onto the remote's audio. The calendar account's own address catches it,
  // and the channel keeps its number.
  it("names nobody when the calendar flags the wrong attendee", () => {
    expect(
      participantHumanIds((database) => {
        database.exec(`
          INSERT INTO humans (id, email) VALUES
            ('mads', ''),
            ('mads-calendar', 'mads.verlin@nordwerk.example');
          INSERT INTO calendars (id, source)
            VALUES ('calendar-1', 'mads.verlin@nordwerk.example');
          INSERT INTO sessions (id, owner_user_id, event_id)
            VALUES ('session-1', 'mads', 'event-1');
          INSERT INTO session_participants (session_id, human_id, email)
            VALUES ('session-1', 'mads-calendar', 'mads.verlin@nordwerk.example');
        `);
        database
          .prepare(
            "INSERT INTO events (id, calendar_id, participants_json) VALUES ('event-1', 'calendar-1', ?)",
          )
          .run(
            JSON.stringify([
              {
                email: "tom.kestroll@nordwerk.example",
                is_current_user: true,
              },
              {
                email: "mads.verlin@nordwerk.example",
                is_current_user: false,
              },
            ]),
          );
      }),
    ).toEqual([]);
  });

  it("does not treat a non-address calendar account as an address", () => {
    expect(
      participantHumanIds((database) => {
        seedGemesseneForm(database, { isCurrentUser: false });
        database.exec(
          "UPDATE calendars SET source = 'Subscribed Calendars' WHERE id = 'calendar-1'",
        );
      }),
    ).toEqual(["mads-calendar", "tom"]);
  });
});

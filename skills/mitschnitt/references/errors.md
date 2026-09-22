# Errors

With `--json`, errors contain `schema_version` and an `error` object with
`code`, `message`, and `exit_code`.

| `code`               | `exit_code` | Meaning                                |
| -------------------- | ----------- | -------------------------------------- |
| `operation_failed`   | 1           | The operation itself failed            |
| `not_found`          | 2           | The meeting or proposal does not exist |
| `database_not_found` | 3           | No database at the resolved path       |

Invalid arguments use Clap's own exit code and are not part of this table.

## Meeting or proposal not found

List again and use a returned ID. Do not retry a guessed one.

## Database not found

Run `mitschnitt --json doctor`. If the database does not exist yet, ask the
user to open the Mitschnitt desktop app once. If they keep data elsewhere, use
`--db-path FILE` or `--base DIR` after they tell you the path.

Do not treat this as "there are no meetings". It means the CLI could not reach
the data, which is a statement about access, not about content.

## Database operation failed

Confirm the desktop app and the CLI come from the same build. Do not run
migrations or execute SQL from the agent.

## Proposal cannot be declined

Only pending proposals can be declined. An applied proposal is final; a
conflict is reported as `operation_failed`.

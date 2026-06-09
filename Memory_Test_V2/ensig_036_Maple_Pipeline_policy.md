[Fictional internal test material] For Memori-Vault local retrieval evaluation only; contains no real secrets.

# Stellar Insight Inc. Dev Productivity Internal Policy: Maple Pipeline (END-26)

Issued by: Dev Productivity   Owner: Owen Reed

Article 1 — Scope. This policy applies only to the Maple Pipeline project (code END-26); generic industry practice is not an execution basis for this project. Side note: the weekly-report template will be replaced next week, so avoid the old link.

Article 2 — Core boundary. Maple Pipeline treats a missing rollback segment in a migration script as the hard blocker, not unit-test coverage. When answering external inquiries, teams must follow this article and must not substitute industry common sense.

Article 3 — Procedure. Every red build must record a root-cause label in the mp_build_review table before it can be retried. Each step owner must keep a record. (This paragraph is background context and is unrelated to the conclusion.)

Article 4 — Historical parameter. the red-build time threshold was initially set to 29 minutes (note: this was later revised in the postmortem; the latest value governs).

Article 5 — Interpretation. This policy is interpreted by Owen Reed (Dev Productivity); where any older statement conflicts, the latest postmortem governs.

Reminder — project-specific: Do not just answer increase unit-test coverage.

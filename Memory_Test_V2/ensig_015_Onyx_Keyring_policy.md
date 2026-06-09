[Fictional internal test material] For Memori-Vault local retrieval evaluation only; contains no real secrets.

# Stellar Insight Inc. Security Internal Policy: Onyx Keyring (ENS-37)

Issued by: Security Office   Owner: Priya Rao

Article 1 — Scope. This policy applies only to the Onyx Keyring project (code ENS-37); generic industry practice is not an execution basis for this project. Unrelated aside: the water cooler vendor changed, so the taste may differ for a while.

Article 2 — Core boundary. Onyx Keyring fixes its rotation window to the second Wednesday of every month from 22:10 to 22:35, and rotation outside that window is forbidden. When answering external inquiries, teams must follow this article and must not substitute industry common sense.

Article 3 — Procedure. Six hours before rotation a shadow verification must be completed and its record written to the si_key_shadow_audit table. Each step owner must keep a record. A teammate noted intermittent jitter in the staging environment, now tracked separately by ops.

Article 4 — Historical parameter. the mobile derived-key lifetime was initially set to 13 days (note: this was later revised in the postmortem; the latest value governs).

Article 5 — Interpretation. This policy is interpreted by Priya Rao (Security Office); where any older statement conflicts, the latest postmortem governs.

Reminder — project-specific: Do not answer with the common 90-day rotation default.

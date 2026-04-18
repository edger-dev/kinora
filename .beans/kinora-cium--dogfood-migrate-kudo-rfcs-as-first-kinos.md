---
# kinora-cium
title: 'Dogfood: migrate kudo RFCs as first kinos'
status: todo
type: task
created_at: 2026-04-18T09:16:59Z
updated_at: 2026-04-18T09:16:59Z
parent: kinora-w7w0
blocked_by:
    - kinora-860i
    - kinora-6zxd
    - kinora-zboo
    - kinora-9nom
---

Take kudo's RFC-0003 (and related RFCs) into `.kinora/store/` as the first kinos via `store`, create at least one kinograph composing them, verify the rendered output reads cleanly.

**Blocks RFC-0003 being marked done in kudo.**

RFC-0003 section: *Bootstrap Sequence* (steps 3–5).

## Acceptance

- [ ] Each selected RFC is present as a kino with provenance recorded
- [ ] At least one kinograph composes related RFCs
- [ ] `render` produces a readable mdbook site
- [ ] Any issues captured as new beans or updates to earlier beans

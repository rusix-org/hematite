# ADR 0002: License proposal

Status: proposed; Aaron's approval required. No license adopted.
Date: 2026-10-25

## Recommendation

Use **MPL-2.0** for original kernel code, host tooling, and documentation,
without the Exhibit B secondary-license exclusion. If ADR 0001 selects an
existing project, preserve its file-specific licenses and contribution rules;
this proposal cannot relicense upstream code.

The intended tradeoff is sharing distributed improvements to covered files
while permitting reuse alongside separately licensed components. Mozilla's
[license text](https://www.mozilla.org/en-US/MPL/2.0/) provides the source
distribution terms and contributor patent grant; its
[FAQ](https://www.mozilla.org/en-US/MPL/2.0/FAQ/) explains file-level copyleft
and the conditions for combining with GPL works. This is not blanket
compatibility approval for an arbitrary dependency graph.

This also aligns with Asterinas's general license, although its
[COPYRIGHT](https://raw.githubusercontent.com/asterinas/asterinas/main/COPYRIGHT)
explicitly allows file/directory exceptions. Check the pinned revision before
adopting any component.

## Alternatives

| Choice | Reason to choose it | Cost relative to proposal |
| --- | --- | --- |
| `MIT OR Apache-2.0` | Permissive reuse with recipient choice; Apache includes an explicit patent grant | No requirement to share distributed modifications; MIT choice does not itself contain Apache's express patent terms |
| `GPL-2.0-only` | Require GPL terms for distribution of derivative works as a whole | More restrictive combination/distribution terms; all dependencies need review against that choice |
| MPL-2.0 | File-level reciprocity and explicit patent terms | Track covered files, notices, and source availability when distributing binaries |

References: [MIT text](https://opensource.org/license/mit),
[Apache-2.0 text](https://www.apache.org/licenses/LICENSE-2.0),
[GPL v2 text](https://opensource.org/license/gpl-2.0), consulted 2026-10-25.

## Adoption after approval

Add the exact license text, matching SPDX metadata, and a dependency/provenance
inventory before introducing dependencies. Record license, source revision,
required notices, and review status per component, including build/test inputs.
Keep external test userspace and Linux oracle artifacts under their own terms.
The clean-room and provenance rules remain in [AGENTS.md](../../AGENTS.md)
regardless of license; licensing does not authorize copying Linux source.

Approval record: pending Aaron's explicit selection and date.

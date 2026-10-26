# CPU topology source review

Date: 2026-10-26. Independent AI review of the first implementation prerequisite
in design 0014. This is not human approval or a complete soundness audit. The
reviewer performs no builds or guest runs; execution evidence belongs to the
integration record.

## Review scope

Inspect bounded safe decoding of CPU and PSCI metadata, its malformed-input
fixtures, and extraction through the existing ARM DTB borrow. The result is
descriptive data, not a CPU-start capability. No CPU_ON, secondary execution,
per-CPU scheduler change or SMP correctness claim belongs to this increment.

Relevant requirements are complete FDT bounds/structure validation, unambiguous
CPU hierarchy and parent-cell widths, no duplicate CPU identities, accepted
enabled affinities limited to the declared profile, and explicit PSCI conduit/
compatibility checks. Unsupported or malformed data must not return a partially
usable topology. Core extraction must retain the established independent DTB
window bound and return owned data without extending a raw-memory borrow.

## Verdict

No blocking source findings for this descriptive-metadata increment. Reviewed
`cpu_topology.rs`, its fixture module, the platform integration, and the hosted
QEMU-tree check. This verdict does not authorize secondary CPU execution.

The parser bounds the header, reservation entries, structure and string blocks;
rejects overlapping or truncated components; and limits depth and property
counts. Checked slice access and bounded indexing cover malformed input. Node
ordering, duplicate property names, direct CPU hierarchy, parent cell widths,
unit-address/reg agreement and duplicate affinities are checked before returning
owned metadata. Accepted identities normalize to `[0]` or `[0, 1]`; unsupported
descriptions are rejected as a whole. Disabled CPUs are rejected rather than
silently removed from the profile.

PSCI requires modern compatibility and the HVC conduit when present; the
two-CPU profile also requires PSCI enable methods on both CPUs. Optional legacy
function properties must agree with the declared standard IDs. Review requested
rejection of misplaced legacy-only `arm,psci` descriptions as well as modern
ones. The final implementation and root/nested/duplicate-node fixtures include
that refinement. Advertised PSCI 1.0 remains firmware-description metadata,
not a successful runtime version query.

Core integration reuses the existing independently bounded 2 MiB DTB window
and the same immutable blob borrow used for memory discovery. It adds no raw
pointer access or lifetime extension. Parsing returns a copied owned value;
`None` denotes unsupported or malformed topology and grants no SMP authority.
The memory parser still independently gates memory discovery. Thus topology
failure is not claimed to reject the entire boot.

Fixtures cover truncations, byte mutations with accepted-output invariants,
header overlap, malformed structure endings, reservation overflow, hierarchy,
identity and PSCI ambiguity. The hosted check links the actual kernel parser,
requires one/two-CPU QEMU trees to normalize correctly with advertised PSCI 1.0,
and requires three CPUs to return `Unsupported`. It records commands, raw
output and hashes, and fails on missing checks or unexpected execution output.
These are source-reviewed test contracts; executed results are recorded by the
integrator. Generated device trees do not demonstrate secondary execution.

## Primary references

Consulted 2026-10-26; no Linux implementation source was consulted or copied.

- [Devicetree flattened format](https://devicetree-specification.readthedocs.io/en/stable/flattened-format.html):
  big-endian header, bounded component blocks, token structure and boot CPU ID.
- [Devicetree device-node requirements](https://devicetree-specification.readthedocs.io/en/stable/devicenodes.html):
  root CPU hierarchy and CPU identification properties.
- Design 0014 and its independent review define this project's intentionally
  narrow two-CPU PSCI/HVC profile; pinned QEMU output supplies configuration
  evidence, not proof that any firmware operation succeeded.

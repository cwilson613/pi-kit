# kernel-composition/documentation - Delta Spec

## ADDED Requirements

### Requirement: Product documentation distinguishes implemented composition boundaries

Architecture, operator, installation, security, and extension documentation must
distinguish the full integration host, reduced kernel artifact, host services,
signed core components, shipped content, and SDK extensions. Claims of capability
or trust parity must be backed by executable evidence for the named artifact and
distribution channel; planned behavior must be labeled as planned.

#### Scenario: Kernel and core architecture is documented
Given the reduced kernel and full product have different executable capabilities
When architecture and operator documentation are validated
Then each artifact's supported operations and component boundaries match executable evidence
And a conformance probe is not presented as production provider-backed execution

#### Scenario: Distribution channels have different composition
Given native archives are full-product while a supported channel is host-only
When installation and security guidance is rendered
Then each channel states its exact component inventory, trust boundary, and typed unavailable behavior
And no host-only channel inherits a full-product readiness claim

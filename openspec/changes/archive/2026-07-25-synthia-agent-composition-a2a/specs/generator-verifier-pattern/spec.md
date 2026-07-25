# Spec: generator-verifier-pattern

## ADDED Requirements

### Requirement: GeneratorVerifier struct
`GeneratorVerifier` SHALL compose two `AgentHandle` instances:
- `generator: Arc<AgentHandle>` — the generator
- `verifier: Arc<AgentHandle>` — the verifier
- `max_rounds: usize` — maximum loop iterations
- `pass_fn: fn(&str) -> bool` — PASS判定 function

#### Scenario: compose generator and verifier
- **WHEN** a `GeneratorVerifier` is created with a generator, verifier, max rounds, and pass function
- **THEN** it holds references to both agent handles and the configuration for the generate-verify loop

### Requirement: GeneratorVerifier.run
`GeneratorVerifier.run(task)` SHALL:
1. Create `gen_tool = agent_as_tool(generator)`
2. Create `ver_tool = agent_as_tool(verifier)`
3. Loop up to `max_rounds`:
   a. `output = gen_tool.call(task + feedback)`
   b. `verdict = ver_tool.call(output)`
   c. If `pass_fn(verdict)`: return `Ok(output)`
   d. `feedback = verdict`
4. Return `Err(MaxRoundsExceeded)` if loop exhausts

#### Scenario: generate until verification passes
- **WHEN** `gv.run(task)` is called and the verifier returns PASS within max rounds
- **THEN** the generator output that passed verification is returned

#### Scenario: exceed max rounds
- **WHEN** `gv.run(task)` is called and the verifier never returns PASS within max rounds
- **THEN** `MaxRoundsExceeded` error is returned

### Requirement: GeneratorVerifier supports A2A
The generator and verifier SHALL be allowed to be remote agents (via `SendMessage` Tool), not limited to local `AgentHandle` instances.

#### Scenario: use remote agent as verifier
- **WHEN** a `GeneratorVerifier` is configured with a remote agent as the verifier
- **THEN** the verify step invokes the remote agent via A2A protocol and the loop proceeds normally

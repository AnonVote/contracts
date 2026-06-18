/**
 * AnonVote Soroban Service
 *
 * TypeScript service for invoking the AnonVote Soroban smart contract from
 * the AnonVote/core backend.
 *
 * STATUS: Contract written (contracts/anonvote/src/lib.rs) — needs deployment.
 * The manageData-based stellarService is the active blockchain layer.
 * This service is ready to wire once the Soroban contract is deployed.
 *
 * TO ACTIVATE:
 * 1. Build the contract:
 *      cd contracts/anonvote && cargo build --target wasm32-unknown-unknown --release
 * 2. Deploy to testnet:
 *      stellar contract deploy --wasm target/wasm32-unknown-unknown/release/anonvote.wasm --network testnet
 * 3. Initialize:
 *      stellar contract invoke --id <CONTRACT_ID> --network testnet -- initialize --admin <PUBLIC_KEY>
 * 4. Set SOROBAN_CONTRACT_ID=<CONTRACT_ID> in backend/.env
 * 5. Call the helpers below from ballotEngine, identityManager, privacyEngine, resultEngine
 */

import * as StellarSdk from "stellar-sdk";

const SOROBAN_RPC_TESTNET = "https://soroban-testnet.stellar.org";
const SOROBAN_RPC_MAINNET = "https://rpc.stellar.org";

// ── Error codes matching ContractError enum in lib.rs ─────────────────────────

export enum SorobanErrorCode {
  AdminUnauthorized      = 1,
  AlreadyInitialized     = 2,
  NotInitialized         = 3,
  BallotNotFound         = 4,
  BallotAlreadyExists    = 5,
  ResultAlreadyPublished = 6,
  CounterOverflow        = 7,
  InvalidBallotHash      = 8,
  // Non-contract errors
  SimulationFailed       = 100,
  TransactionFailed      = 101,
  NetworkError           = 102,
  NotConfigured          = 103,
}

const ERROR_MESSAGES: Record<SorobanErrorCode, string> = {
  [SorobanErrorCode.AdminUnauthorized]:      "Caller is not the contract admin",
  [SorobanErrorCode.AlreadyInitialized]:     "Contract already initialized",
  [SorobanErrorCode.NotInitialized]:         "Contract not initialized",
  [SorobanErrorCode.BallotNotFound]:         "Ballot does not exist on-chain",
  [SorobanErrorCode.BallotAlreadyExists]:    "Ballot already recorded by a different admin",
  [SorobanErrorCode.ResultAlreadyPublished]: "A different result hash is already published for this ballot",
  [SorobanErrorCode.CounterOverflow]:        "Counter has reached u32::MAX",
  [SorobanErrorCode.InvalidBallotHash]:      "Ballot hash must not be empty",
  [SorobanErrorCode.SimulationFailed]:       "Transaction simulation failed",
  [SorobanErrorCode.TransactionFailed]:      "Transaction submission failed",
  [SorobanErrorCode.NetworkError]:           "Network or RPC error",
  [SorobanErrorCode.NotConfigured]:          "Contract ID or secret key not configured",
};

// ── Public interfaces ─────────────────────────────────────────────────────────

export interface SorobanConfig {
  stellarSecretKey: string;
  stellarNetwork: "testnet" | "mainnet";
  contractId: string;
  rpcServer?: Pick<StellarSdk.SorobanRpc.Server, "getEvents"> | undefined;
}

export enum BallotState {
  Active          = "Active",
  ResultPublished = "ResultPublished",
}

export interface BallotStateSnapshot {
  tokens_issued: number;
  votes_cast: number;
  result_hash: string | null;
  created_at: number;
  admin: string;
  state: BallotState;
}

export interface SorobanInvokeResult {
  txHash: string;
  success: boolean;
  returnValue?: unknown;
  errorCode?: SorobanErrorCode;
  errorMessage?: string;
}

export type SorobanAuditEventType =
  | "ballot_created"
  | "token_issued"
  | "vote_cast"
  | "result_published"
  | "counter_overflow"
  | "admin_rotated";

export interface SorobanEventFilter {
  eventType?: SorobanAuditEventType | string;
  ballotIdHash?: string;
  startTime?: number;
  endTime?: number;
}

export interface SorobanEventData {
  id: string;
  pagingToken?: string | undefined;
  ledger: number;
  ledgerClosedAt?: string | undefined;
  timestamp?: number | undefined;
  contractId?: string | undefined;
  eventType: SorobanAuditEventType | string;
  ballotIdHash?: string | undefined;
  count?: number | undefined;
  createdAt?: number | undefined;
  admin?: string | undefined;
  previousAdmin?: string | undefined;
  newAdmin?: string | undefined;
  resultHash?: string | undefined;
  topics: unknown[];
  value: unknown;
}

// ── Internal helpers ──────────────────────────────────────────────────────────

function getRpcUrl(network: string): string {
  return network === "mainnet" ? SOROBAN_RPC_MAINNET : SOROBAN_RPC_TESTNET;
}

function getNetworkPassphrase(network: string): string {
  return network === "mainnet"
    ? StellarSdk.Networks.PUBLIC
    : StellarSdk.Networks.TESTNET;
}

function getRpcServer(network: string): StellarSdk.SorobanRpc.Server {
  return new StellarSdk.SorobanRpc.Server(getRpcUrl(network), {
    allowHttp: false,
  });
}

function makeError(code: SorobanErrorCode): Pick<SorobanInvokeResult, "errorCode" | "errorMessage"> {
  return { errorCode: code, errorMessage: ERROR_MESSAGES[code] };
}

/**
 * Parse a Soroban contract error code out of a simulation error string.
 * Contract errors are surfaced as "Error(Contract, #N)" in the XDR diagnostics.
 */
function parseContractErrorCode(errorText: string): SorobanErrorCode | undefined {
  // Soroban encodes contract errors as "Error(Contract, #<code>)"
  const match = errorText.match(/Error\(Contract,\s*#(\d+)\)/);
  if (match) {
    const code = parseInt(match[1], 10);
    if (code in SorobanErrorCode) return code as SorobanErrorCode;
  }
  return undefined;
}

const EVENT_SYMBOL_TO_TYPE: Record<string, SorobanAuditEventType> = {
  blt_crtd: "ballot_created",
  ballot_created: "ballot_created",
  tok_issd: "token_issued",
  token_issued: "token_issued",
  vote_cast: "vote_cast",
  res_pub: "result_published",
  result_published: "result_published",
  cnt_ovflw: "counter_overflow",
  counter_overflow: "counter_overflow",
  adm_rotd: "admin_rotated",
  admin_rotated: "admin_rotated",
};

const EVENT_TYPE_TO_SYMBOL: Record<SorobanAuditEventType, string> = {
  ballot_created: "blt_crtd",
  token_issued: "tok_issd",
  vote_cast: "vote_cast",
  result_published: "res_pub",
  counter_overflow: "cnt_ovflw",
  admin_rotated: "adm_rotd",
};

const SOROBAN_EVENT_PAGE_LIMIT = 100;
const SOROBAN_EVENT_MAX_PAGES = 25;

function normalizeEventType(eventType: unknown): SorobanAuditEventType | string {
  const key = String(eventType ?? "").trim();
  return EVENT_SYMBOL_TO_TYPE[key] ?? key;
}

function parseLedgerClosedAt(ledgerClosedAt: unknown): number | undefined {
  if (!ledgerClosedAt) return undefined;
  const parsed = Date.parse(String(ledgerClosedAt));
  return Number.isNaN(parsed) ? undefined : Math.floor(parsed / 1000);
}

function normalizeTimeFilter(timestamp: number): number {
  return timestamp > 9999999999 ? Math.floor(timestamp / 1000) : timestamp;
}

function scValToNativeSafe(value: unknown): unknown {
  if (!value) return value;
  try {
    return StellarSdk.scValToNative(value as any);
  } catch {
    return value;
  }
}

function getEventTopics(event: any): unknown[] {
  const topics = event.topic ?? event.topics ?? [];
  return Array.isArray(topics) ? topics.map(scValToNativeSafe) : [];
}

function getEventValue(event: any): unknown {
  return scValToNativeSafe(event.value);
}

function getEventTypeFromTopics(topics: unknown[]): SorobanAuditEventType | string {
  const eventTopic = topics.find((topic) => {
    const value = String(topic ?? "");
    return value !== "audit" && EVENT_SYMBOL_TO_TYPE[value] !== undefined;
  });
  return normalizeEventType(eventTopic ?? "");
}

function getTupleValue(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [value];
}

export function parseSorobanEvent(event: unknown): SorobanEventData {
  const raw = event as any;
  const topics = getEventTopics(raw);
  const value = getEventValue(raw);
  const tuple = getTupleValue(value);
  const eventType = getEventTypeFromTopics(topics);
  const timestamp = parseLedgerClosedAt(raw.ledgerClosedAt);

  const parsed: SorobanEventData = {
    id: String(raw.id ?? raw.pagingToken ?? `${raw.ledger ?? ""}:${topics.join(":")}`),
    pagingToken: raw.pagingToken,
    ledger: Number(raw.ledger ?? 0),
    ledgerClosedAt: raw.ledgerClosedAt,
    timestamp,
    contractId: raw.contractId,
    eventType,
    topics,
    value,
  };

  switch (eventType) {
    case "ballot_created":
      parsed.ballotIdHash = String(tuple[0] ?? "");
      parsed.createdAt = Number(tuple[1] ?? 0);
      parsed.admin = tuple[2] !== undefined ? String(tuple[2]) : undefined;
      break;
    case "token_issued":
    case "vote_cast":
      parsed.ballotIdHash = String(tuple[0] ?? "");
      parsed.count = Number(tuple[1] ?? 0);
      break;
    case "result_published":
      parsed.ballotIdHash = String(tuple[0] ?? "");
      parsed.resultHash = String(tuple[1] ?? "");
      break;
    case "counter_overflow":
      parsed.ballotIdHash = String(tuple[0] ?? "");
      break;
    case "admin_rotated":
      parsed.previousAdmin = tuple[0] !== undefined ? String(tuple[0]) : undefined;
      parsed.newAdmin = tuple[1] !== undefined ? String(tuple[1]) : undefined;
      break;
  }

  return parsed;
}

function matchesEventFilter(event: SorobanEventData, filter: SorobanEventFilter): boolean {
  if (filter.eventType && event.eventType !== normalizeEventType(filter.eventType)) {
    return false;
  }
  if (filter.ballotIdHash && event.ballotIdHash !== filter.ballotIdHash) {
    return false;
  }
  if (
    filter.startTime !== undefined &&
    event.timestamp !== undefined &&
    event.timestamp < normalizeTimeFilter(filter.startTime)
  ) {
    return false;
  }
  if (
    filter.endTime !== undefined &&
    event.timestamp !== undefined &&
    event.timestamp > normalizeTimeFilter(filter.endTime)
  ) {
    return false;
  }
  return true;
}

function buildTopicFilter(eventType?: string): string[][] | undefined {
  if (!eventType) return undefined;
  const normalized = normalizeEventType(eventType);
  const symbol = EVENT_TYPE_TO_SYMBOL[normalized as SorobanAuditEventType] ?? eventType;

  try {
    const auditTopic = StellarSdk.nativeToScVal("audit", { type: "symbol" as any }).toXDR("base64");
    const eventTopic = StellarSdk.nativeToScVal(symbol, { type: "symbol" as any }).toXDR("base64");
    return [[auditTopic], [eventTopic]];
  } catch {
    return undefined;
  }
}

// ── Core invoke / read ────────────────────────────────────────────────────────

/**
 * Invoke a method on the deployed AnonVote Soroban contract.
 * Parses contract error codes from simulation and surfaces them in the result.
 */
export async function invokeContract(
  config: SorobanConfig,
  method: string,
  args: { value: unknown; type: string }[],
): Promise<SorobanInvokeResult> {
  if (!config.stellarSecretKey || !config.contractId) {
    console.warn(`[Soroban] ${method}: not configured, skipping`);
    return { txHash: "", success: false, ...makeError(SorobanErrorCode.NotConfigured) };
  }

  try {
    const keypair = StellarSdk.Keypair.fromSecret(config.stellarSecretKey);
    const server   = getRpcServer(config.stellarNetwork);
    const account  = await server.getAccount(keypair.publicKey());

    const scArgs   = args.map(({ value, type }) =>
      StellarSdk.nativeToScVal(value, { type: type as any }),
    );

    const contract  = new StellarSdk.Contract(config.contractId);
    const operation = contract.call(method, ...scArgs);

    const tx = new StellarSdk.TransactionBuilder(account, {
      fee: StellarSdk.BASE_FEE,
      networkPassphrase: getNetworkPassphrase(config.stellarNetwork),
    })
      .addOperation(operation)
      .setTimeout(30)
      .build();

    const simulation = await server.simulateTransaction(tx);

    if (StellarSdk.SorobanRpc.Api.isSimulationError(simulation)) {
      const contractCode = parseContractErrorCode(simulation.error);
      const code    = contractCode ?? SorobanErrorCode.SimulationFailed;
      const message = contractCode
        ? ERROR_MESSAGES[contractCode]
        : simulation.error;
      console.error(`[Soroban] ${method} simulation failed — code ${code}: ${message}`);
      return { txHash: "", success: false, errorCode: code, errorMessage: message };
    }

    const preparedTx = StellarSdk.SorobanRpc.assembleTransaction(
      tx,
      simulation,
    ).build();

    preparedTx.sign(keypair);
    const sendResult = await server.sendTransaction(preparedTx);

    if (sendResult.status === "ERROR") {
      console.error(`[Soroban] ${method} send failed:`, sendResult.errorResult);
      return { txHash: "", success: false, ...makeError(SorobanErrorCode.TransactionFailed) };
    }

    const txHash = sendResult.hash;
    let getResult = await server.getTransaction(txHash);
    let attempts  = 0;

    while (
      getResult.status === StellarSdk.SorobanRpc.Api.GetTransactionStatus.NOT_FOUND &&
      attempts < 10
    ) {
      await new Promise((r) => setTimeout(r, 1500));
      getResult = await server.getTransaction(txHash);
      attempts++;
    }

    if (getResult.status === StellarSdk.SorobanRpc.Api.GetTransactionStatus.SUCCESS) {
      const returnValue = getResult.returnValue
        ? StellarSdk.scValToNative(getResult.returnValue)
        : undefined;
      console.log(`[Soroban] ${method} succeeded — tx: ${txHash}`);
      return { txHash, success: true, returnValue };
    }

    console.error(`[Soroban] ${method} transaction failed:`, getResult);
    return { txHash: "", success: false, ...makeError(SorobanErrorCode.TransactionFailed) };
  } catch (err) {
    console.error(`[Soroban] ${method} network error:`, err);
    return { txHash: "", success: false, ...makeError(SorobanErrorCode.NetworkError) };
  }
}

/**
 * Read contract data without submitting a transaction (view call / simulation only).
 * Returns { value, errorCode, errorMessage } so callers can distinguish "not found"
 * from "network error".
 */
export async function readContract(
  config: SorobanConfig,
  method: string,
  args: { value: unknown; type: string }[],
): Promise<{ value: unknown | null; errorCode?: SorobanErrorCode; errorMessage?: string }> {
  if (!config.contractId) {
    console.warn(`[Soroban] ${method}: no contract ID, skipping read`);
    return { value: null, ...makeError(SorobanErrorCode.NotConfigured) };
  }

  try {
    const keypair = config.stellarSecretKey
      ? StellarSdk.Keypair.fromSecret(config.stellarSecretKey)
      : StellarSdk.Keypair.random();

    const server  = getRpcServer(config.stellarNetwork);
    const account = await server.getAccount(keypair.publicKey());

    const scArgs  = args.map(({ value, type }) =>
      StellarSdk.nativeToScVal(value, { type: type as any }),
    );

    const contract  = new StellarSdk.Contract(config.contractId);
    const operation = contract.call(method, ...scArgs);

    const tx = new StellarSdk.TransactionBuilder(account, {
      fee: StellarSdk.BASE_FEE,
      networkPassphrase: getNetworkPassphrase(config.stellarNetwork),
    })
      .addOperation(operation)
      .setTimeout(30)
      .build();

    const simulation = await server.simulateTransaction(tx);

    if (StellarSdk.SorobanRpc.Api.isSimulationError(simulation)) {
      const contractCode = parseContractErrorCode(simulation.error);
      const code    = contractCode ?? SorobanErrorCode.SimulationFailed;
      const message = contractCode ? ERROR_MESSAGES[contractCode] : simulation.error;
      console.error(`[Soroban] ${method} read failed — code ${code}: ${message}`);
      return { value: null, errorCode: code, errorMessage: message };
    }

    if (
      StellarSdk.SorobanRpc.Api.isSimulationSuccess(simulation) &&
      simulation.result?.retval
    ) {
      return { value: StellarSdk.scValToNative(simulation.result.retval) };
    }

    return { value: null };
  } catch (err) {
    console.error(`[Soroban] ${method} read error:`, err);
    return { value: null, ...makeError(SorobanErrorCode.NetworkError) };
  }
}

/**
 * Query Soroban RPC contract events and return structured audit events.
 *
 * RPC is narrowed to this contract and, when possible, the requested audit
 * event topic. Ballot and time range filters are then applied client-side so
 * callers can combine filters without manual iteration.
 */
export async function sorobanFilterEvents(
  config: SorobanConfig,
  filter: SorobanEventFilter = {},
): Promise<SorobanEventData[]> {
  if (!config.contractId) {
    console.warn("[Soroban] sorobanFilterEvents: no contract ID, skipping event query");
    return [];
  }

  try {
    const server = config.rpcServer ?? getRpcServer(config.stellarNetwork);
    const events: SorobanEventData[] = [];
    let cursor: string | undefined;
    let pages = 0;

    do {
      const eventFilter: any = {
        type: "contract",
        contractIds: [config.contractId],
      };
      const topics = buildTopicFilter(filter.eventType);
      if (topics) eventFilter.topics = topics;

      const response = await (server as any).getEvents({
        startLedger: cursor ? undefined : 0,
        filters: [eventFilter],
        pagination: {
          cursor,
          limit: SOROBAN_EVENT_PAGE_LIMIT,
        },
      });

      const pageEvents = Array.isArray(response.events) ? response.events : [];
      for (const rawEvent of pageEvents) {
        const parsed = parseSorobanEvent(rawEvent);
        if (matchesEventFilter(parsed, filter)) {
          events.push(parsed);
        }
      }

      const lastEvent = pageEvents[pageEvents.length - 1];
      const nextCursor = response.cursor
        ?? (pageEvents.length === SOROBAN_EVENT_PAGE_LIMIT ? lastEvent?.pagingToken : undefined);
      cursor = nextCursor && nextCursor !== cursor ? nextCursor : undefined;
      pages++;
    } while (cursor && pages < SOROBAN_EVENT_MAX_PAGES);

    return events;
  } catch (err) {
    console.error("[Soroban] sorobanFilterEvents query failed:", err);
    return [];
  }
}

// ── AnonVote contract helpers ─────────────────────────────────────────────────

/**
 * Record a ballot creation on-chain.
 * Idempotent: if the same ballot was already recorded by this admin, returns the
 * existing txHash (empty string for idempotent success without a new tx).
 */
export async function sorobanRecordBallot(
  config: SorobanConfig,
  ballotIdHash: string,
): Promise<string> {
  if (!config.contractId) return "";
  const caller = StellarSdk.Keypair.fromSecret(config.stellarSecretKey).publicKey();
  const result = await invokeContract(config, "record_ballot", [
    { value: caller, type: "address" },
    { value: ballotIdHash, type: "string" },
  ]);
  if (!result.success && result.errorCode !== undefined) {
    console.error(
      `[Soroban] sorobanRecordBallot failed — ${SorobanErrorCode[result.errorCode]}: ${result.errorMessage}`,
    );
  }
  return result.txHash;
}

/**
 * Record a token issuance on-chain.
 */
export async function sorobanRecordToken(
  config: SorobanConfig,
  ballotIdHash: string,
): Promise<string> {
  if (!config.contractId) return "";
  const caller = StellarSdk.Keypair.fromSecret(config.stellarSecretKey).publicKey();
  const result = await invokeContract(config, "record_token", [
    { value: caller, type: "address" },
    { value: ballotIdHash, type: "string" },
  ]);
  if (!result.success) {
    if (result.errorCode === SorobanErrorCode.BallotNotFound) {
      console.error(
        `[Soroban] sorobanRecordToken: ballot ${ballotIdHash} not found on-chain — BallotNotFound`,
      );
    } else if (result.errorCode !== undefined) {
      console.error(
        `[Soroban] sorobanRecordToken failed — ${SorobanErrorCode[result.errorCode]}: ${result.errorMessage}`,
      );
    }
  }
  return result.txHash;
}

/**
 * Record a vote cast on-chain.
 */
export async function sorobanRecordVote(
  config: SorobanConfig,
  ballotIdHash: string,
): Promise<string> {
  if (!config.contractId) return "";
  const caller = StellarSdk.Keypair.fromSecret(config.stellarSecretKey).publicKey();
  const result = await invokeContract(config, "record_vote", [
    { value: caller, type: "address" },
    { value: ballotIdHash, type: "string" },
  ]);
  if (!result.success) {
    if (result.errorCode === SorobanErrorCode.BallotNotFound) {
      console.error(
        `[Soroban] sorobanRecordVote: ballot ${ballotIdHash} not found on-chain — BallotNotFound`,
      );
    } else if (result.errorCode !== undefined) {
      console.error(
        `[Soroban] sorobanRecordVote failed — ${SorobanErrorCode[result.errorCode]}: ${result.errorMessage}`,
      );
    }
  }
  return result.txHash;
}

/**
 * Record a result publication on-chain.
 * Handles ResultAlreadyPublished idempotency: if the same hash is already
 * published, treats the call as success and returns the existing txHash as "".
 */
export async function sorobanRecordResult(
  config: SorobanConfig,
  ballotIdHash: string,
  resultHash: string,
): Promise<string> {
  if (!config.contractId) return "";
  const caller = StellarSdk.Keypair.fromSecret(config.stellarSecretKey).publicKey();
  const result = await invokeContract(config, "record_result", [
    { value: caller, type: "address" },
    { value: ballotIdHash, type: "string" },
    { value: resultHash, type: "string" },
  ]);

  if (!result.success) {
    if (result.errorCode === SorobanErrorCode.ResultAlreadyPublished) {
      // Check if the on-chain hash matches ours (idempotent re-record)
      const { value: onChainHash } = await readContract(config, "get_result_hash", [
        { value: ballotIdHash, type: "string" },
      ]);
      if (onChainHash === resultHash) {
        console.log(
          `[Soroban] sorobanRecordResult: result already published with matching hash — treating as success`,
        );
        return "";
      }
      console.error(
        `[Soroban] sorobanRecordResult: conflicting result already published for ballot ${ballotIdHash}`,
      );
    } else if (result.errorCode !== undefined) {
      console.error(
        `[Soroban] sorobanRecordResult failed — ${SorobanErrorCode[result.errorCode]}: ${result.errorMessage}`,
      );
    }
  }
  return result.txHash;
}

/**
 * Rotate the contract admin to a new address.
 * Must be called by the current admin.
 */
export async function sorobanRotateAdmin(
  config: SorobanConfig,
  newAdminPublicKey: string,
): Promise<string> {
  if (!config.contractId) return "";
  const caller = StellarSdk.Keypair.fromSecret(config.stellarSecretKey).publicKey();
  const result = await invokeContract(config, "rotate_admin", [
    { value: caller, type: "address" },
    { value: newAdminPublicKey, type: "address" },
  ]);
  if (!result.success && result.errorCode !== undefined) {
    console.error(
      `[Soroban] sorobanRotateAdmin failed — ${SorobanErrorCode[result.errorCode]}: ${result.errorMessage}`,
    );
  }
  return result.txHash;
}

/**
 * Read on-chain audit counts for a ballot (view call — no transaction).
 */
export async function sorobanGetAuditCounts(
  config: SorobanConfig,
  ballotIdHash: string,
): Promise<{
  tokensIssued: number | null;
  votesCast: number | null;
  isConsistent: boolean;
} | null> {
  if (!config.contractId) return null;
  const [tokensRes, votesRes, consistentRes] = await Promise.all([
    readContract(config, "get_tokens_issued", [{ value: ballotIdHash, type: "string" }]),
    readContract(config, "get_votes_cast",    [{ value: ballotIdHash, type: "string" }]),
    readContract(config, "is_consistent",     [{ value: ballotIdHash, type: "string" }]),
  ]);
  return {
    tokensIssued:  tokensRes.value    as number | null,
    votesCast:     votesRes.value     as number | null,
    isConsistent: (consistentRes.value as boolean) ?? false,
  };
}

/**
 * Get complete ballot state snapshot (single read call).
 */
export async function sorobanGetBallotState(
  config: SorobanConfig,
  ballotIdHash: string,
): Promise<BallotStateSnapshot | null> {
  if (!config.contractId) return null;
  const { value } = await readContract(config, "get_ballot_state", [
    { value: ballotIdHash, type: "string" },
  ]);
  return value as BallotStateSnapshot | null;
}

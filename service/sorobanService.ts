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

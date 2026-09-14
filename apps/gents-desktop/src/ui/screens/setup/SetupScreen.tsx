/* First run, from the Startup designs: choose where the agent lives,
   name it, watch it come online, then progressively connect one inference
   provider. Provider/model guidance comes from the versioned Rust contract;
   the final canonical documents are committed in one operator transaction. */
import { useEffect, useRef, useState } from "react";
import {
  ArrowLeft,
  ArrowRight,
  Circle,
  CircleCheck,
  KeyRound,
  Moon,
  Orbit,
  Server,
  Sparkles,
  Sun,
  Wifi,
} from "lucide-react";
import type {
  DesktopClientSnapshot,
  ManagedServerAuthorityInput,
  InferenceAuthMethod,
  InferenceDiscoveryResult,
  InferenceModelOption,
  InferenceModelRecommendation,
  InferenceProviderId,
  InferenceSetupCatalog,
} from "@source-inc/gents-desktop-client";
import { Button } from "@gents/ui/components/button";
import { Input } from "@gents/ui/components/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@gents/ui/components/select";
import { Spinner } from "@gents/ui/components/spinner";
import { cn } from "@gents/ui/lib/utils";
import { ScrollArea } from "@gents/ui/components/scroll-area";
import {
  projectStartupLoadingStatus,
  type DesktopStartupPhase,
  type LoadingStepState,
} from "../../../lib/loadingStatus";
import type { Shell } from "@/hooks/useShell";
import { setupStewardPatches } from "@/lib/setupSteward";
import { isMobileTauriShell } from "../../../lib/shellPlatform";
import { AgentAvatar } from "@/screens/AgentAvatar";
import { applyTheme, themePreference } from "@/theme";
import { Mark } from "@/app/Mark";
import { openExternalUrl } from "../../../lib/externalLinks";
import { watchProviderLoginUrl, type OauthProvider } from "@/lib/providerLogin";
import {
  ManagedRuntimeAuthorityPicker,
  ManagedRuntimeAuthorityReview,
  authorityForPreset,
} from "@/components/ManagedRuntimeAuthority";
import {
  authoritiesEqual,
  type ManagedRuntimePreset,
} from "@/lib/managedRuntimeAuthority";
import {
  currentInferenceDiscovery,
  inferenceDiscoveryKey,
} from "@/lib/providerDiscovery";
import { buildInferenceSetupPlan } from "@/lib/inferenceSetupPersistence";
import {
  InferenceModelControls,
  recommendedInferenceSettings,
  validateInferenceSettings,
  type InferenceSettingsDraft,
} from "../inference/InferenceModelControls";

type Step =
  | "welcome"
  | "remote"
  | "agent"
  | "authority"
  | "authority-review"
  | "starting"
  | "inference"
  | "ready";

type ProviderId = InferenceProviderId;
type InferenceStage = "provider" | "connect" | "model" | "review";
type ConnectionDraft = {
  authMethod: InferenceAuthMethod;
  endpoint: string;
  apiKey: string;
};

const PROVIDER_VISUALS: Record<ProviderId, { icon: typeof Server; logo: string }> = {
  openai: { icon: KeyRound, logo: "/logos/openai.svg" },
  anthropic: { icon: Sparkles, logo: "/logos/claude.svg" },
  grok: { icon: Orbit, logo: "/logos/grok.svg" },
  local: { icon: Server, logo: "/logos/ollama.svg" },
  openrouter: { icon: KeyRound, logo: "/logos/openrouter.svg" },
};

const authLabel = (method: InferenceAuthMethod) =>
  ({
    chat_gpt_oauth: "ChatGPT sign-in",
    api_key: "API key",
    claude_oauth: "Claude sign-in",
    grok_oauth: "Grok sign-in",
    optional_api_key: "Endpoint + optional key",
  })[method];

const oauthProviderFor = (method: InferenceAuthMethod): OauthProvider | null =>
  method === "chat_gpt_oauth"
    ? "openai"
    : method === "claude_oauth"
      ? "anthropic"
      : method === "grok_oauth"
        ? "grok"
        : null;

function Frame({ children }: { children: React.ReactNode }) {
  const [theme, setTheme] = useState(themePreference);
  const flip = () => {
    const next = theme === "dark" ? "light" : "dark";
    applyTheme(next);
    setTheme(next);
  };
  return (
    <ScrollArea
      className="viewport-frame relative bg-background text-foreground"
      data-testid="setup-screen"
    >
      <div className="px-8">
        {/* anchored a fixed way down, not centred: a step can grow or shrink without moving its title */}
        <div className="mx-auto w-full max-w-xl pt-[22vh] pb-16">{children}</div>
        <Button
          variant="ghost"
          size="icon-sm"
          className="absolute right-6 bottom-6"
          aria-label="Toggle theme"
          onClick={flip}
        >
          {theme === "dark" ? <Sun /> : <Moon />}
        </Button>
      </div>
    </ScrollArea>
  );
}

function Title({ children, note }: { children: React.ReactNode; note: string }) {
  return (
    <div className="mb-6">
      <h1 className="font-heading text-2xl font-medium text-heading">{children}</h1>
      <p className="mt-1.5 text-sm text-muted-foreground">{note}</p>
    </div>
  );
}

function Option({
  selected,
  onSelect,
  title,
  hint,
  icon: Icon,
  logo,
  testId,
}: {
  selected: boolean;
  onSelect: () => void;
  title: string;
  hint?: string;
  icon: typeof Server;
  logo?: string;
  testId?: string;
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={selected}
      data-testid={testId}
      onClick={onSelect}
      className={cn(
        "flex w-full items-center gap-3 rounded-2xl border bg-raised px-4 py-3.5 text-left transition-shadow",
        selected
          ? "border-brand ring-1 ring-brand"
          : "border-border/60 hover:bg-accent",
      )}
    >
      {selected ? (
        <CircleCheck className="size-4 shrink-0 text-foreground" />
      ) : (
        <Circle className="size-4 shrink-0 text-muted-foreground" />
      )}
      <span className="min-w-0 flex-1">
        <span className="block text-sm font-medium">{title}</span>
        {hint && (
          <span className="block truncate text-xs text-muted-foreground">{hint}</span>
        )}
      </span>
      {logo ? (
        <img src={logo} alt="" className="size-5 shrink-0 object-contain dark:invert" />
      ) : (
        <Icon className="size-5 shrink-0 text-heading" />
      )}
    </button>
  );
}

function Nav({
  onBack,
  next,
  nextLabel = "Next",
  busy,
  disabled,
}: {
  onBack?: () => void;
  next?: () => void;
  nextLabel?: string;
  busy?: boolean;
  disabled?: boolean;
}) {
  return (
    <div className="mt-8 flex items-center justify-between">
      {onBack ? (
        <button
          type="button"
          onClick={onBack}
          className="inline-flex items-center gap-1 text-sm text-muted-foreground hover:text-foreground"
        >
          <ArrowLeft className="size-3.5" /> Back
        </button>
      ) : (
        <span />
      )}
      {next && (
        <Button
          variant="brand"
          onClick={next}
          disabled={disabled || busy}
          data-testid="setup-next"
        >
          {busy ? <Spinner /> : null} {nextLabel} <ArrowRight />
        </Button>
      )}
    </div>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="grid gap-1">
      <span className="text-xs text-muted-foreground">{label}</span>
      {children}
    </label>
  );
}

const stepIcon = (state: LoadingStepState | null) =>
  state === "complete" ? (
    <CircleCheck className="size-4 text-muted-foreground" />
  ) : state === "active" ? (
    <Spinner className="text-foreground" />
  ) : (
    <span className="size-1.5 rounded-full bg-border" />
  );

export function SetupScreen({
  shell,
  onDone,
  initialStep = "welcome",
}: {
  shell: Shell;
  onDone: (snapshot: DesktopClientSnapshot) => void;
  initialStep?: Step;
}) {
  const [step, setStep] = useState<Step>(initialStep);
  const allowLocal = !isMobileTauriShell();
  const api = shell.api;
  const [where, setWhere] = useState<"local" | "remote">(
    allowLocal ? "local" : "remote",
  );
  const [address, setAddress] = useState("");
  const [name, setName] = useState("Forge");
  const [homeRoot, setHomeRoot] = useState<string | null>(
    api.managedServerStatus ? null : (shell.snapshot?.bootstrap.initToolRoot ?? null),
  );
  const [authorityPreset, setAuthorityPreset] =
    useState<ManagedRuntimePreset>("full-home");
  const [selectedDirectory, setSelectedDirectory] = useState<string | null>(null);
  const [authorityError, setAuthorityError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [phase, setPhase] = useState<Exclude<DesktopStartupPhase, "ready">>(
    "checking-managed-server",
  );
  const [inferenceStage, setInferenceStage] = useState<InferenceStage>("provider");
  const [catalog, setCatalog] = useState<InferenceSetupCatalog | null>(null);
  const [provider, setProvider] = useState<ProviderId>("openai");
  const [connections, setConnections] = useState<
    Partial<Record<ProviderId, ConnectionDraft>>
  >({});
  const [signedIn, setSignedIn] = useState<Partial<Record<ProviderId, string>>>({});
  const [authUrl, setAuthUrl] = useState<string | null>(null);
  const root = shell.snapshot?.bootstrap.defaultAgentHome ?? "~/.gents";
  const authority = homeRoot
    ? authorityForPreset(authorityPreset, homeRoot, selectedDirectory)
    : null;

  useEffect(() => {
    if (step !== "authority" || !api.managedServerStatus || homeRoot) return;
    const pending = api.managedServerStatus();
    if (!pending) return;
    void pending
      .then((status) => {
        if (status.suggestedToolRoot) setHomeRoot(status.suggestedToolRoot);
      })
      .catch((cause) =>
        setAuthorityError(cause instanceof Error ? cause.message : String(cause)),
      );
  }, [api, homeRoot, step]);
  const [discovery, setDiscovery] = useState<InferenceDiscoveryResult | null>(null);
  const [modelSearch, setModelSearch] = useState("");
  const [model, setModel] = useState("");
  const [manualModel, setManualModel] = useState(false);
  const [settings, setSettings] = useState<InferenceSettingsDraft | null>(null);
  const [selectedRecommendation, setSelectedRecommendation] =
    useState<InferenceModelRecommendation | null>(null);
  const [customize, setCustomize] = useState(false);
  const discoveryRevision = useRef(0);
  const currentDiscoveryKey = useRef("");
  const connection = connections[provider];
  const providerOption = catalog?.providers.find((option) => option.id === provider);

  const finishProvisioning = async () => {
    const next = await api.fetchDesktopSnapshot();
    const nextDeployment = next.client?.deployments[0];
    const steward = nextDeployment ? setupStewardPatches(nextDeployment) : [];
    if (steward.length && nextDeployment) {
      await api.patchConfigComponents({
        agentDid: nextDeployment.agentDid,
        patches: steward,
      });
    }
    await shell.refreshSnapshot();
    setStep("inference");
  };

  useEffect(() => {
    if (step !== "inference" || catalog) return;
    void api
      .getInferenceSetupCatalog()
      .then((next) => {
        setCatalog(next);
        setConnections((current) => {
          const initialized = { ...current };
          for (const option of next.providers) {
            initialized[option.id] ??= {
              authMethod: option.defaultAuthMethod,
              endpoint: option.defaultEndpoint,
              apiKey: "",
            };
          }
          return initialized;
        });
        if (next.providers[0]) setProvider(next.providers[0].id);
      })
      .catch((cause) =>
        setError(cause instanceof Error ? cause.message : String(cause)),
      );
  }, [api, catalog, step]);

  const createAgent = async () => {
    const agentName = name.trim() || "Local Agent";
    if (!authority) {
      setAuthorityError(
        homeRoot
          ? "Choose and validate an existing directory."
          : "The user home directory is still being resolved.",
      );
      setStep("authority");
      return;
    }
    setBusy(true);
    setError(null);
    setStep("starting");
    setPhase("checking-managed-server");
    try {
      if (api.startManagedServer) {
        const status = await api.startManagedServer(agentName, authority);
        const confirmed: ManagedServerAuthorityInput | null =
          status.effectiveToolCeiling
            ? {
                toolCeiling: status.effectiveToolCeiling,
                toolRoot: status.effectiveToolRoot,
              }
            : null;
        if (!confirmed || !authoritiesEqual(confirmed, authority)) {
          throw new Error(
            "The managed runtime started with different authority than the reviewed settings.",
          );
        }
      }
      setPhase("loading-configuration");
      await shell.onInitLocalRuntime(agentName);
      setPhase("starting-client");
      if (api.commitManagedServerAutoStart) {
        await api.commitManagedServerAutoStart(agentName);
      }
      if (api.managedServerStatus) {
        const deadline = Date.now() + 30_000;
        while (Date.now() < deadline) {
          const status = await api.managedServerStatus();
          if (status.pairingReady) break;
          await new Promise((resolve) => window.setTimeout(resolve, 250));
        }
        const status = await api.managedServerStatus();
        if (!status.pairingReady) {
          throw new Error(
            "The hosted agent started, but secure background pairing is not ready.",
          );
        }
      }
      await finishProvisioning();
    } catch (e) {
      setPhase("managed-server-error");
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };
  const enrol = async () => {
    setBusy(true);
    setError(null);
    setStep("starting");
    setPhase("starting-client");
    try {
      await api.requestStatusEnrollment(address.trim());
      await finishProvisioning();
    } catch (e) {
      setPhase("client-error");
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };
  const signIn = async () => {
    if (!connection) return;
    const oauthProvider = oauthProviderFor(connection.authMethod);
    if (!oauthProvider) return;
    setBusy(true);
    setError(null);
    setAuthUrl(null);
    let unlisten = () => {};
    try {
      unlisten = await watchProviderLoginUrl(oauthProvider, setAuthUrl);
      const snapshot = await api.fetchDesktopSnapshot();
      const agentDid =
        shell.snapshot?.client?.deployments[0]?.agentDid ??
        snapshot.client?.deployments[0]?.agentDid;
      if (!agentDid) throw new Error("No agent to sign in");
      const result =
        oauthProvider === "openai"
          ? await api.codexLogin(agentDid)
          : oauthProvider === "anthropic"
            ? await api.claudeLogin(agentDid)
            : await api.grokLogin(agentDid);
      setSignedIn((current) => ({ ...current, [provider]: result.credentialId }));
      invalidateDiscovery();
      setAuthUrl(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      unlisten();
      setBusy(false);
    }
  };

  const cancelSignIn = () => {
    const oauthProvider = connection ? oauthProviderFor(connection.authMethod) : null;
    if (oauthProvider === "openai") void api.cancelCodexLogin();
    else if (oauthProvider === "anthropic") void api.cancelClaudeLogin();
    else if (oauthProvider === "grok") void api.cancelGrokLogin();
  };

  const invalidateDiscovery = () => {
    discoveryRevision.current += 1;
    currentDiscoveryKey.current = "";
    setDiscovery(null);
    setModel("");
    setManualModel(false);
    setSelectedRecommendation(null);
    setSettings(null);
    setCustomize(false);
  };

  const updateConnection = (changes: Partial<ConnectionDraft>) => {
    setConnections((current) => ({
      ...current,
      [provider]: { ...current[provider]!, ...changes },
    }));
    invalidateDiscovery();
    setError(null);
  };

  const discoverModels = async () => {
    if (!connection) return;
    setBusy(true);
    setError(null);
    const snapshot = await api.fetchDesktopSnapshot();
    const agentDid =
      shell.snapshot?.client?.deployments[0]?.agentDid ??
      snapshot.client?.deployments[0]?.agentDid;
    if (!agentDid) {
      setBusy(false);
      setError("No agent to configure");
      return;
    }
    const requestKey = inferenceDiscoveryKey(
      ++discoveryRevision.current,
      provider,
      connection.authMethod,
      connection.endpoint,
    );
    currentDiscoveryKey.current = requestKey;
    try {
      const result = await api.discoverInferenceModels({
        requestKey,
        agentDid,
        provider,
        authMethod: connection.authMethod,
        endpoint: connection.endpoint,
        apiKey: connection.apiKey.trim() || null,
      });
      const current = currentInferenceDiscovery(currentDiscoveryKey.current, result);
      if (!current) return;
      setDiscovery(current);
      // Discovery supplies choices, but the user makes the one model decision.
      // Even a one-item catalog is never accepted implicitly.
      setModel("");
      setSelectedRecommendation(null);
      setSettings(null);
      setInferenceStage("model");
    } catch (cause) {
      if (currentDiscoveryKey.current !== requestKey) return;
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  };

  const chooseModel = (option: InferenceModelOption) => {
    setModel(option.advertised.model_name);
    setSelectedRecommendation(option.recommendation);
    setSettings(recommendedInferenceSettings(option.recommendation));
    setCustomize(false);
  };

  const describeManualModel = async () => {
    if (!connection || !model.trim()) return;
    setBusy(true);
    setError(null);
    try {
      const recommendation = await api.getInferenceModelRecommendation({
        provider,
        authMethod: connection.authMethod,
        modelName: model.trim(),
        displayName: null,
        contextWindow: null,
        maxOutputTokens: null,
        reasoningEfforts: null,
      });
      setSelectedRecommendation(recommendation);
      setSettings(recommendedInferenceSettings(recommendation));
      setInferenceStage("review");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  };

  const persistInference = async () => {
    if (!connection || !discovery || !selectedRecommendation || !settings)
      throw new Error("Complete provider discovery and model selection first");
    const settingsError = validateInferenceSettings(selectedRecommendation, settings);
    if (settingsError) throw new Error(settingsError);
    const snapshot = await api.fetchDesktopSnapshot();
    const deployment =
      shell.snapshot?.client?.deployments[0] ?? snapshot.client?.deployments[0];
    if (!deployment) throw new Error("No agent to configure");

    const plan = buildInferenceSetupPlan({
      deployment,
      provider,
      apiKey: connection.apiKey,
      oauth: oauthProviderFor(connection.authMethod) !== null,
      discovery,
      model,
      recommendation: selectedRecommendation,
      settings,
    });
    await api.applyConfigComponents({ document: plan.document });

    await shell.refreshSnapshot();
    return plan;
  };

  const waitForSelectedBehavior = async (
    profileId: string,
    defaultBehaviorId: string | null,
  ) => {
    for (let attempt = 0; attempt < 20; attempt += 1) {
      const snapshot = await api.fetchDesktopSnapshot();
      const deployment = snapshot.client?.deployments.find(
        (candidate) =>
          candidate.agentDid === shell.selectedDeployment?.agentDid ||
          candidate.agentDid === snapshot.client?.deployments[0]?.agentDid,
      );
      const bound = deployment?.behaviorConfigs.find(
        (behavior) =>
          behavior.behavior_id === defaultBehaviorId &&
          behavior.inference_profile_id === profileId,
      );
      const ready = deployment?.behaviorReadiness.behaviors.some(
        (status) => status.state === "ready" && status.behaviorId === defaultBehaviorId,
      );
      if (bound && ready) return snapshot;
      await new Promise((resolve) => window.setTimeout(resolve, 250));
    }
    throw new Error(
      "Configuration was saved, but the managed runtime has not reported the Setup behavior active yet.",
    );
  };

  const saveInference = async () => {
    setBusy(true);
    setError(null);
    try {
      const { profileId, defaultBehaviorId } = await persistInference();
      await waitForSelectedBehavior(profileId, defaultBehaviorId);
      setStep("ready");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  };

  const pickProvider = (id: ProviderId) => {
    if (busy) cancelSignIn();
    setProvider(id);
    setAuthUrl(null);
    setError(null);
    invalidateDiscovery();
  };
  if (step === "welcome") {
    return (
      <Frame>
        <Mark className="mb-6 h-6 text-ink" />
        <Title note="Gents runs agents whose every step is a document. Start one here, or connect to one that already runs.">
          Let’s get set up
        </Title>
        <div className="grid gap-3">
          {allowLocal && (
            <Option
              selected={where === "local"}
              onSelect={() => setWhere("local")}
              title="Local agent"
              hint="Create an agent on this Mac."
              icon={Server}
            />
          )}
          <Option
            selected={where === "remote"}
            onSelect={() => setWhere("remote")}
            title="Remote connect"
            hint="Join a Gents server someone else runs."
            icon={Wifi}
          />
        </div>
        <Nav next={() => setStep(where === "local" ? "agent" : "remote")} />
      </Frame>
    );
  }
  if (step === "remote") {
    return (
      <Frame>
        <Title note="The server's status address. Its admin approves the enrolment.">
          Connect to a server
        </Title>
        <Input
          value={address}
          onChange={(e) => setAddress(e.target.value)}
          placeholder="https://gents.example.net:8787"
          autoFocus
        />
        {error && <p className="mt-3 text-sm text-destructive">{error}</p>}
        <Nav
          onBack={() => setStep("welcome")}
          next={enrol}
          nextLabel="Request access"
          busy={busy}
          disabled={!address.trim()}
        />
      </Frame>
    );
  }
  if (step === "agent") {
    return (
      <Frame>
        <div className="mb-4 flex items-center gap-1">
          <AgentAvatar name={name} className="size-9" />
        </div>
        <Title note="Its name is how it appears everywhere; its home is where its documents live.">
          Configure your agent
        </Title>
        <div className="rounded-2xl border border-border/60 bg-raised p-4">
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            aria-label="Agent name"
            autoFocus
          />
          <p className="mt-3 font-mono text-xs text-muted-foreground">
            root: <span className="text-foreground">{root}</span>
          </p>
        </div>
        {error && <p className="mt-3 text-sm text-destructive">{error}</p>}
        <Nav
          onBack={() => setStep("welcome")}
          next={() => setStep("authority")}
          disabled={!name.trim()}
        />
      </Frame>
    );
  }
  if (step === "authority") {
    return (
      <Frame>
        <Title note="This is the most access any behavior can receive. You can make individual behaviors narrower later.">
          What can the hosted agent do on this computer?
        </Title>
        {homeRoot ? (
          <ManagedRuntimeAuthorityPicker
            home={homeRoot}
            preset={authorityPreset}
            selectedDirectory={selectedDirectory}
            onPresetChange={setAuthorityPreset}
            onDirectoryChange={setSelectedDirectory}
            validateRoot={api.validateManagedServerRoot}
            error={authorityError}
            onError={setAuthorityError}
          />
        ) : (
          <div className="grid gap-3">
            <p className="flex items-center gap-2 text-sm text-muted-foreground">
              <Spinner /> Resolving your home directory…
            </p>
            {authorityError ? (
              <>
                <p className="text-sm text-destructive">{authorityError}</p>
                <Button
                  variant="outline"
                  className="justify-self-start"
                  onClick={() => {
                    setAuthorityError(null);
                    void api
                      .managedServerStatus?.()
                      .then((status) => {
                        if (status?.suggestedToolRoot) {
                          setHomeRoot(status.suggestedToolRoot);
                        }
                      })
                      .catch((cause) =>
                        setAuthorityError(
                          cause instanceof Error ? cause.message : String(cause),
                        ),
                      );
                  }}
                >
                  Try again
                </Button>
              </>
            ) : null}
          </div>
        )}
        <Nav
          onBack={() => setStep("agent")}
          next={() => {
            if (!authority) {
              setAuthorityError("Choose and validate an existing directory.");
              return;
            }
            setStep("authority-review");
          }}
          disabled={!authority}
        />
      </Frame>
    );
  }
  if (step === "authority-review" && authority) {
    return (
      <Frame>
        <Title note="The managed runtime will start with exactly these host limits.">
          Review access
        </Title>
        <ManagedRuntimeAuthorityReview authority={authority} />
        {authorityError ? (
          <p className="mt-3 text-sm text-destructive">{authorityError}</p>
        ) : null}
        <Nav
          onBack={() => setStep("authority")}
          next={createAgent}
          nextLabel="Start hosted agent"
          busy={busy}
        />
      </Frame>
    );
  }
  if (step === "starting") {
    const status = projectStartupLoadingStatus(phase, true);
    const steps: [string, LoadingStepState | null][] = [
      ["Restore hosted agent", status.managedServerState],
      ["Read saved connections", status.connectionState],
      ["Start secure client", status.clientState],
    ];
    const saying: Record<string, string> = {
      "checking-managed-server": "Waking the hosted agent…",
      "loading-configuration": "Teaching the gossip network some manners…",
      "starting-client": "Turning the secure client on…",
    };
    return (
      <Frame>
        <h1 className="font-heading text-2xl font-medium text-heading">
          {status.failed ? status.title : "Startup"}
        </h1>
        <p className="mt-3 flex items-center gap-2 text-sm text-muted-foreground">
          {status.failed ? null : <Spinner className="text-foreground" />}
          {saying[phase] ?? status.currentLabel}
        </p>
        <ol className="mt-6 grid gap-2">
          {steps.map(([label, state], i) =>
            state === "pending" ? null : (
              <li
                key={label}
                className="flex items-center gap-3 rounded-2xl border border-border/60 bg-raised px-4 py-3 text-sm animate-in fade-in-0 slide-in-from-bottom-1 duration-300 fill-mode-both"
              >
                <span className="font-mono text-[11px] text-muted-foreground">
                  0{i + 1}.
                </span>
                <span className="flex-1">{label}</span>
                {stepIcon(state)}
              </li>
            ),
          )}
        </ol>
        {error && (
          <div className="mt-4 grid gap-3">
            <p className="text-sm text-destructive">{error}</p>
            <Button
              variant="brand"
              onClick={() => {
                setError(null);
                setStep(where === "local" ? "authority-review" : "remote");
              }}
            >
              Try again
            </Button>
          </div>
        )}
      </Frame>
    );
  }
  if (step === "ready") {
    const agentName =
      shell.selectedDeployment?.agentPrincipal.displayName ??
      (name.trim() || "your agent");
    const providerTitle = providerOption?.displayName ?? "Inference";
    return (
      <Frame>
        <Mark className="mb-6 h-6 text-ink" />
        <Title
          note={`${providerTitle} is connected. The first message starts a conversation with ${agentName}.`}
        >
          You’re in
        </Title>
        <p className="text-sm text-muted-foreground">
          Send a message to begin. You can add another provider later from the agent’s
          inference settings.
        </p>
        {error && <p className="mt-3 text-sm text-destructive">{error}</p>}
        <Nav
          next={async () => {
            setBusy(true);
            try {
              const snapshot = await api.fetchDesktopSnapshot();
              onDone(snapshot);
            } catch (e) {
              setError(e instanceof Error ? e.message : String(e));
              setBusy(false);
            }
          }}
          nextLabel="Start chatting"
          busy={busy}
        />
      </Frame>
    );
  }
  const advertised = discovery?.models.find(
    (option) => option.advertised.model_name === model,
  )?.advertised;
  const filteredModels =
    discovery?.models.filter((option) => {
      const query = modelSearch.trim().toLocaleLowerCase();
      return (
        !query ||
        option.advertised.model_name.toLocaleLowerCase().includes(query) ||
        option.advertised.display_name?.toLocaleLowerCase().includes(query)
      );
    }) ?? [];
  const oauthProvider = connection ? oauthProviderFor(connection.authMethod) : null;
  const connectionReady = Boolean(
    connection?.endpoint.trim() &&
    (oauthProvider
      ? signedIn[provider]
      : connection.authMethod === "optional_api_key" || connection.apiKey.trim()),
  );

  if (inferenceStage === "provider") {
    return (
      <Frame>
        <Title note="Choose one provider first. Connection and model choices come next.">
          Choose an inference provider
        </Title>
        {catalog ? (
          <div
            className="grid grid-cols-2 gap-3"
            role="radiogroup"
            aria-label="Inference provider"
          >
            {catalog.providers.map((option) => {
              const visual = PROVIDER_VISUALS[option.id];
              return (
                <Option
                  key={option.id}
                  selected={provider === option.id}
                  onSelect={() => pickProvider(option.id)}
                  title={option.displayName}
                  hint={option.description}
                  icon={visual.icon}
                  logo={visual.logo}
                  testId={`setup-provider-${option.id}`}
                />
              );
            })}
          </div>
        ) : (
          <p className="flex items-center gap-2 text-sm text-muted-foreground">
            <Spinner /> Loading provider options…
          </p>
        )}
        {error && <p className="mt-3 text-sm text-destructive">{error}</p>}
        <Nav
          onBack={initialStep === "inference" ? undefined : () => setStep("agent")}
          next={() => setInferenceStage("connect")}
          disabled={!catalog || !connection}
        />
      </Frame>
    );
  }

  if (inferenceStage === "connect") {
    const authOptions = providerOption?.authOptions ?? [];
    return (
      <Frame>
        <Title note="Connect first. Gents will then ask this exact provider for its advertised models.">
          Connect {providerOption?.displayName ?? "provider"}
        </Title>
        <div className="grid gap-4 rounded-2xl border border-border/60 bg-raised p-4 text-sm">
          {connection && authOptions.length > 1 ? (
            <Field label="Connection method">
              <Select
                items={authOptions.map((option) => ({
                  value: option.method,
                  label: option.displayName,
                }))}
                value={connection.authMethod}
                onValueChange={(next) => {
                  if (!next) return;
                  const option = authOptions.find((item) => item.method === next);
                  updateConnection({
                    authMethod: next as InferenceAuthMethod,
                    endpoint: option?.defaultEndpoint ?? connection.endpoint,
                    apiKey: "",
                  });
                }}
              >
                <SelectTrigger className="w-full" autoFocus>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {authOptions.map((option) => (
                    <SelectItem key={option.method} value={option.method}>
                      {option.displayName}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>
          ) : null}
          {oauthProvider ? (
            <div className="grid gap-3">
              {signedIn[provider] ? (
                <p className="flex items-center gap-2">
                  <CircleCheck className="size-4" />
                  Connected credential{" "}
                  <span className="font-mono text-xs">{signedIn[provider]}</span>
                </p>
              ) : (
                <div className="flex items-center justify-between gap-3">
                  <p className="text-muted-foreground">
                    {authLabel(connection!.authMethod)}
                  </p>
                  <span className="flex gap-2">
                    {busy ? (
                      <Button variant="outline" onClick={cancelSignIn}>
                        Cancel
                      </Button>
                    ) : null}
                    <Button variant="brand" disabled={busy} onClick={signIn}>
                      {busy ? <Spinner /> : null}
                      {busy ? "Waiting…" : "Sign in"}
                    </Button>
                  </span>
                </div>
              )}
              {authUrl ? (
                <button
                  type="button"
                  className="justify-self-start text-xs underline"
                  onClick={() => void openExternalUrl(authUrl)}
                >
                  Open the sign-in page
                </button>
              ) : null}
            </div>
          ) : (
            <>
              <Field
                label={
                  connection?.authMethod === "optional_api_key"
                    ? "API key (optional)"
                    : "API key"
                }
              >
                <Input
                  type="password"
                  value={connection?.apiKey ?? ""}
                  autoFocus={authOptions.length <= 1}
                  onChange={(event) => updateConnection({ apiKey: event.target.value })}
                  placeholder="Stored only when you save"
                />
              </Field>
              <Field label="Endpoint">
                <Input
                  value={connection?.endpoint ?? ""}
                  className="font-mono"
                  onChange={(event) =>
                    updateConnection({ endpoint: event.target.value })
                  }
                />
              </Field>
            </>
          )}
        </div>
        {error && <p className="mt-3 text-sm text-destructive">{error}</p>}
        <Nav
          onBack={() => setInferenceStage("provider")}
          next={discoverModels}
          nextLabel="Connect and find models"
          busy={busy}
          disabled={!connectionReady}
        />
      </Frame>
    );
  }

  if (inferenceStage === "model") {
    return (
      <Frame>
        <Title note="These names are advertised by the connected provider. Gents will not silently substitute another model.">
          Choose a model
        </Title>
        {discovery?.failure ? (
          <div className="mb-4 rounded-xl border border-destructive/30 p-3">
            <p className="text-sm text-destructive">{discovery.failure.message}</p>
            <Button
              className="mt-3"
              variant="outline"
              onClick={() => setInferenceStage("connect")}
            >
              Retry connection
            </Button>
          </div>
        ) : null}
        {discovery?.models.length ? (
          <div className="grid gap-3">
            <Input
              value={modelSearch}
              onChange={(event) => setModelSearch(event.target.value)}
              placeholder="Search advertised models"
              aria-label="Search advertised models"
              autoFocus
            />
            <div
              role="listbox"
              aria-label="Advertised models"
              className="max-h-64 overflow-y-auto rounded-xl border border-border/60 p-1"
            >
              {filteredModels.map((option) => (
                <button
                  key={option.advertised.model_name}
                  type="button"
                  role="option"
                  aria-selected={model === option.advertised.model_name}
                  className={cn(
                    "block w-full rounded-lg px-3 py-2 text-left text-sm",
                    model === option.advertised.model_name
                      ? "bg-accent text-foreground"
                      : "hover:bg-accent/60",
                  )}
                  onClick={() => chooseModel(option)}
                >
                  <span className="block font-medium">
                    {option.advertised.display_name ?? option.advertised.model_name}
                  </span>
                  {option.advertised.display_name ? (
                    <span className="block font-mono text-xs text-muted-foreground">
                      {option.advertised.model_name}
                    </span>
                  ) : null}
                </button>
              ))}
            </div>
          </div>
        ) : discovery?.manualEntryAllowed ? (
          <div className="grid gap-3">
            <p className="text-sm text-muted-foreground">
              Model discovery is unavailable. Manual entry is enabled as an explicit
              fallback and will be saved exactly as entered.
            </p>
            <Field label="Manual model identifier">
              <Input
                value={model}
                onChange={(event) => {
                  setModel(event.target.value);
                  setManualModel(true);
                }}
                placeholder="Exact served model ID"
                autoFocus
              />
            </Field>
          </div>
        ) : (
          <p className="text-sm text-muted-foreground">No discovery result.</p>
        )}
        {error && <p className="mt-3 text-sm text-destructive">{error}</p>}
        <Nav
          onBack={() => setInferenceStage("connect")}
          next={manualModel ? describeManualModel : () => setInferenceStage("review")}
          nextLabel="Review defaults"
          busy={busy}
          disabled={!model.trim() || (!manualModel && !selectedRecommendation)}
        />
      </Frame>
    );
  }

  return (
    <Frame>
      <Title note="Provider facts and Gents recommendations are shown separately. Customize only the controls supported for this model.">
        Review inference
      </Title>
      <div className="mb-4 rounded-2xl border border-border/60 bg-raised p-4 text-sm">
        <p className="font-medium">{model}</p>
        <p className="mt-2 text-xs text-muted-foreground">Provider advertised</p>
        <dl className="mt-1 grid grid-cols-2 gap-2 text-xs">
          <div>
            <dt className="text-muted-foreground">Context window</dt>
            <dd>{advertised?.context_window?.toLocaleString() ?? "Not advertised"}</dd>
          </div>
          <div>
            <dt className="text-muted-foreground">Max output</dt>
            <dd>
              {advertised?.max_output_tokens?.toLocaleString() ?? "Not advertised"}
            </dd>
          </div>
        </dl>
      </div>
      {selectedRecommendation && settings ? (
        <InferenceModelControls
          recommendation={selectedRecommendation}
          value={settings}
          onChange={setSettings}
          expanded={customize}
          onExpandedChange={setCustomize}
        />
      ) : null}
      {error && <p className="mt-3 text-sm text-destructive">{error}</p>}
      <Nav
        onBack={() => setInferenceStage("model")}
        next={saveInference}
        nextLabel="Save and activate"
        busy={busy}
        disabled={!selectedRecommendation || !settings}
      />
    </Frame>
  );
}

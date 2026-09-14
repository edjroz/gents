import { describe, expect, it } from "vitest";
import type {
  InferenceDiscoveryResult,
  InferenceModelRecommendation,
} from "@source-inc/gents-desktop-client";
import { buildInferenceSetupPlan } from "../src/ui/lib/inferenceSetupPersistence";
import { deployment as fixtureDeployment } from "./config-panel-wiring/fixtures";

const recommendation: InferenceModelRecommendation = {
  defaultsVersion: "2026-09-14.1",
  summary: "Fixture defaults",
  contextWindow: null,
  maxOutputTokens: null,
  temperature: { recommended: 1, min: 0, max: 2, step: 0.05 },
  topP: { recommended: 0.95, min: 0, max: 1, step: 0.05 },
  reasoningEffort: null,
  maxConcurrent: { recommended: 1, min: 1, max: null },
};

const discovery: InferenceDiscoveryResult = {
  requestKey: "1:local",
  contractVersion: 1,
  defaultsVersion: "2026-09-14.1",
  requestedEndpoint: "http://workstation-1:8000/v1",
  effectiveEndpoint: "http://workstation-1:8000/v1",
  backendName: "Local server",
  providerKind: "OpenAiCompatible",
  openaiWireApi: "chat_completions",
  reachable: true,
  models: [],
  failure: null,
  manualEntryAllowed: false,
};

describe("inference setup persistence", () => {
  it("plans backend, exact model defaults, and Setup behavior in one document", () => {
    const deployment = structuredClone(fixtureDeployment);
    deployment.inferenceBackends = [
      {
        ...deployment.inferenceBackends[0]!,
        backendId: `${deployment.agentDid}:backend`,
        apiKeyConfigured: false,
        authKind: null,
        probeStatus: null,
      },
    ];
    deployment.inferenceProfiles = [
      {
        ...deployment.inferenceProfiles[0]!,
        backend_id: `${deployment.agentDid}:backend`,
      },
    ];
    deployment.behaviorConfigs = [deployment.behaviorConfigs[0]!];

    const plan = buildInferenceSetupPlan({
      deployment,
      provider: "local",
      apiKey: "",
      oauth: false,
      discovery,
      model: "GLM-5.3-Flash-NVFP4",
      recommendation,
      settings: {
        contextWindow: "",
        maxOutputTokens: "",
        temperature: "1",
        topP: "0.95",
        reasoningEffort: "",
        maxConcurrent: "1",
      },
    });

    expect(plan.document.inference_backends).toEqual([
      expect.objectContaining({
        endpoint: "http://workstation-1:8000/v1",
        auth: { kind: "unauthenticated" },
      }),
    ]);
    expect(plan.document.inference_profiles).toEqual([
      expect.objectContaining({ model_name: "GLM-5.3-Flash-NVFP4" }),
    ]);
    expect(plan.document.inference_sampling).toEqual([
      expect.objectContaining({ temperature: 1, top_p: 0.95 }),
    ]);
    expect(plan.document.agent_behaviors).toEqual([
      expect.objectContaining({
        behavior_id: "default",
        context_id: "context-a",
        inference_profile_id: plan.profileId,
      }),
    ]);
  });
});

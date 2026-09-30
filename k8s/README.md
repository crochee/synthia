# Synthia k8s manifests

Reference Deployment + Service + PodDisruptionBudget pairs for the
two synthia workloads. These are starting points for a cluster
operator — production should overlay an Ingress, a NetworkPolicy,
HPA, and Secrets via your GitOps flow of choice (Argo CD /
Flux / Helmfile).

## Layout

| File | Workload | Notes |
|---|---|---|
| `server.yaml` | `synthia-server` (Rust API) | Deployment + Service + PDB, securityContext=non-root 65532, probes hit `/livez` + `/readyz` |
| `web.yaml`    | `synthia-web` (nginx-fronted SPA) | Deployment + Service + PDB, securityContext=non-root 65532 |

## Required secrets

```bash
# The single Secret consumed by the server Deployment via `envFrom`.
# API keys for OpenAI / Anthropic / etc. plus optional OTLP token.
kubectl create secret generic synthia-secrets \
  --from-literal=openai-api-key=sk-... \
  --from-literal=anthropic-api-key=sk-ant-... \
  --from-literal=synthia-otlp-token=... \
  -n synthia
```

## Apply

```bash
kubectl apply -f k8s/server.yaml
kubectl apply -f k8s/web.yaml
```

## Design points

- `runAsNonRoot: true` + `runAsUser: 65532` aligns the pod's uid
  with the binary shipped by `Dockerfile.server` (and the distroless
  / alpine base image — `nginx:alpine` ships root by default, the
  pod security context drops privileges).
- `seccompProfile: RuntimeDefault` is the k8s 1.25+ default; we
  state it explicitly so a future pod-template edit cannot
  regress it.
- `readOnlyRootFilesystem: true` plus `emptyDir` mounts on `/data`
  and `/tmp` — sqlite memory / journal writes need a writable
  path, but everything else is read-only.
- PDB `minAvailable: 1` so voluntary disruptions (node drains,
  cluster upgrades) never empty the Deployment.
- `maxUnavailable: 0` + `maxSurge: 1` on the RollingUpdate
  strategy — soft placement, capacity never dips mid-rollout.

## What is intentionally NOT here

- **Ingress** — TLS termination, hostname routing, and rate
  limiting are cluster-specific; choose your own controller
  (ingress-nginx / traefik / cloud LB).
- **NetworkPolicy** — namespace-default deny + selective allow is
  cluster-specific.
- **HPA** — autoscaling thresholds depend on observed load; left to
  the operator.
- **PersistentVolumeClaim** — synthia-server is stateless
  (in-memory + jsonl + optional sqlite). Add a PVC only if you
  configure a durable journal path in `config.yaml`.
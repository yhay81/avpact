#!/usr/bin/env bash
set -euo pipefail

export LC_ALL=C

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
result_path="${1:-${root_dir}/benchmark-results.json}"
binary="${root_dir}/target/release/avpact"

for dependency in cargo cmp ffmpeg ffprobe git jq stat timeout uname; do
  command -v "${dependency}" >/dev/null || {
    printf 'missing benchmark dependency: %s\n' "${dependency}" >&2
    exit 1
  }
done

if ! /usr/bin/time --version 2>&1 | grep -qi 'GNU time'; then
  printf 'benchmarks/run.sh requires GNU /usr/bin/time (the Ubuntu runner provides it)\n' >&2
  exit 1
fi

temp_dir="$(mktemp -d)"
trap 'rm -rf "${temp_dir}"' EXIT

workspace="${temp_dir}/workflow"
mkdir -p "${workspace}"
source_media="${workspace}/source.mp4"
output_media="${workspace}/clip.mp4"
recipe="${workspace}/recipe.json"
plan="${workspace}/plan.json"
receipt="${workspace}/receipt.json"

cd "${root_dir}"
cargo build --release --locked

timeout --signal=KILL 60s \
  ffmpeg \
  -hide_banner \
  -loglevel error \
  -nostdin \
  -f lavfi \
  -i testsrc2=size=160x90:rate=10 \
  -f lavfi \
  -i sine=frequency=1000:sample_rate=48000 \
  -t 0.5 \
  -c:v mpeg4 \
  -c:a aac \
  -map_metadata -1 \
  "${source_media}"

jq -n '{
  schema_version: "avpact.recipe/v0.1",
  operation: {
    type: "clip",
    input: "source.mp4",
    output: "clip.mp4",
    start_ms: 100,
    end_ms: 400
  },
  target: "web",
  constraints: {
    overwrite: "deny",
    duration_tolerance_ms: 100,
    max_output_bytes: 10485760,
    max_temporary_bytes: 12582912,
    max_runtime_ms: 30000
  }
}' >"${recipe}"

measure() {
  local metrics="$1"
  local output="$2"
  local diagnostics="$3"
  shift 3

  /usr/bin/time \
    -f '{"wall_seconds": %e, "max_rss_kib": %M, "exit_code": %x}' \
    -o "${metrics}" \
    timeout --signal=KILL 120s "$@" >"${output}" 2>"${diagnostics}"
  jq -e . "${metrics}" >/dev/null
  jq -e . "${output}" >/dev/null
}

catalog_metrics="${temp_dir}/catalog.metrics.json"
catalog_output="${temp_dir}/catalog.json"
catalog_diagnostics="${temp_dir}/catalog.stderr"
schema_metrics="${temp_dir}/schema.metrics.json"
schema_output="${temp_dir}/schema.json"
schema_diagnostics="${temp_dir}/schema.stderr"
inspect_metrics="${temp_dir}/inspect.metrics.json"
inspect_output="${temp_dir}/inspect.json"
inspect_diagnostics="${temp_dir}/inspect.stderr"
plan_metrics="${temp_dir}/plan.metrics.json"
plan_output="${temp_dir}/plan.stdout.json"
plan_diagnostics="${temp_dir}/plan.stderr"
apply_metrics="${temp_dir}/apply.metrics.json"
apply_output="${temp_dir}/apply.stdout.json"
progress_output="${temp_dir}/apply.progress.ndjson"
verify_metrics="${temp_dir}/verify.metrics.json"
verify_output="${temp_dir}/verify.json"
verify_diagnostics="${temp_dir}/verify.stderr"

measure "${catalog_metrics}" "${catalog_output}" "${catalog_diagnostics}" \
  "${binary}" schema --brief --format json
measure "${schema_metrics}" "${schema_output}" "${schema_diagnostics}" \
  "${binary}" schema --document plan --format json
measure "${inspect_metrics}" "${inspect_output}" "${inspect_diagnostics}" \
  "${binary}" inspect "${source_media}" --format json
measure "${plan_metrics}" "${plan_output}" "${plan_diagnostics}" \
  "${binary}" plan "${recipe}" --out "${plan}" --format json

cmp "${plan}" "${plan_output}"
jq -e '
  .schema_version == "avpact.plan/v0.1"
  and .operation.type == "clip"
  and .operation.start_ms == 100
  and .operation.end_ms == 400
  and .operation.duration_ms == 300
  and (.backend.argv | length > 0)
  and (.verification_checks | length > 0)
' "${plan}" >/dev/null

measure "${apply_metrics}" "${apply_output}" "${progress_output}" \
  "${binary}" apply "${plan}" \
  --receipt-out "${receipt}" \
  --progress ndjson \
  --format json

jq -e . "${receipt}" >/dev/null
jq -e -s '
  length > 0
  and all(.[]; .schema_version == "avpact.progress/v0.1")
  and any(.[]; .state == "finished")
' "${progress_output}" >/dev/null
jq -e '
  .schema_version == "avpact.receipt/v0.1"
  and .verification.passed
  and all(.verification.checks[]; .passed)
  and .publication.method == "same_filesystem_hard_link"
' "${receipt}" >/dev/null
test -s "${output_media}"
temporary_output="$(jq -er '.output.temporary_path' "${plan}")"
test ! -e "${temporary_output}"

measure "${verify_metrics}" "${verify_output}" "${verify_diagnostics}" \
  "${binary}" verify "${output_media}" --against "${plan}" --format json
jq -e '
  .schema_version == "avpact.verification/v0.1"
  and .passed
  and all(.checks[]; .passed)
' "${verify_output}" >/dev/null

mkdir -p "$(dirname "${result_path}")"
jq -n \
  --arg generated_at "$(date -u +'%Y-%m-%dT%H:%M:%SZ')" \
  --arg git_sha "$(git rev-parse HEAD)" \
  --arg runner_os "${RUNNER_OS:-Linux}" \
  --arg runner_arch "$(uname -m)" \
  --arg runner_image "${ImageOS:-unknown}" \
  --arg runner_image_version "${ImageVersion:-unknown}" \
  --argjson catalog_output_bytes "$(stat -c '%s' "${catalog_output}")" \
  --argjson catalog_diagnostic_bytes "$(stat -c '%s' "${catalog_diagnostics}")" \
  --argjson schema_output_bytes "$(stat -c '%s' "${schema_output}")" \
  --argjson schema_diagnostic_bytes "$(stat -c '%s' "${schema_diagnostics}")" \
  --argjson inspect_output_bytes "$(stat -c '%s' "${inspect_output}")" \
  --argjson inspect_diagnostic_bytes "$(stat -c '%s' "${inspect_diagnostics}")" \
  --argjson plan_output_bytes "$(stat -c '%s' "${plan_output}")" \
  --argjson plan_diagnostic_bytes "$(stat -c '%s' "${plan_diagnostics}")" \
  --argjson apply_output_bytes "$(stat -c '%s' "${apply_output}")" \
  --argjson progress_output_bytes "$(stat -c '%s' "${progress_output}")" \
  --argjson receipt_bytes "$(stat -c '%s' "${receipt}")" \
  --argjson output_media_bytes "$(stat -c '%s' "${output_media}")" \
  --argjson verify_output_bytes "$(stat -c '%s' "${verify_output}")" \
  --argjson verify_diagnostic_bytes "$(stat -c '%s' "${verify_diagnostics}")" \
  --slurpfile catalog_metrics "${catalog_metrics}" \
  --slurpfile catalog "${catalog_output}" \
  --slurpfile schema_metrics "${schema_metrics}" \
  --slurpfile schema "${schema_output}" \
  --slurpfile inspect_metrics "${inspect_metrics}" \
  --slurpfile inspection "${inspect_output}" \
  --slurpfile plan_metrics "${plan_metrics}" \
  --slurpfile plan "${plan}" \
  --slurpfile apply_metrics "${apply_metrics}" \
  --slurpfile receipt "${receipt}" \
  --slurpfile progress "${progress_output}" \
  --slurpfile verify_metrics "${verify_metrics}" \
  --slurpfile verification "${verify_output}" \
  '{
    schema_version: "avpact.benchmark.v1",
    generated_at: $generated_at,
    git_sha: $git_sha,
    runner: {
      os: $runner_os,
      arch: $runner_arch,
      image: $runner_image,
      image_version: $runner_image_version
    },
    fixture: {
      schema_version: "avpact.benchmark-media.v1",
      generator: "FFmpeg lavfi testsrc2 plus sine",
      requested_duration_ms: 500,
      source_bytes: $inspection[0].source.size_bytes,
      source_sha256: $inspection[0].source.sha256,
      observed_duration_ms: $inspection[0].format.duration_ms,
      streams: [
        $inspection[0].streams[] | {
          index,
          kind,
          codec,
          width,
          height,
          channels
        }
      ]
    },
    backend: {
      name: $plan[0].backend.name,
      version: $plan[0].backend.version,
      configuration: $plan[0].backend.configuration,
      library_versions: $plan[0].backend.library_versions
    },
    measurements: [
      {
        id: "contract_catalog",
        class: "contract",
        process: $catalog_metrics[0],
        output_bytes: $catalog_output_bytes,
        diagnostic_bytes: $catalog_diagnostic_bytes,
        result: {
          schema_version: $catalog[0].schema_version,
          document_count: ($catalog[0].documents | length)
        }
      },
      {
        id: "contract_plan_schema",
        class: "contract",
        process: $schema_metrics[0],
        output_bytes: $schema_output_bytes,
        diagnostic_bytes: $schema_diagnostic_bytes,
        result: {
          draft: $schema[0]."$schema",
          title: $schema[0].title
        }
      },
      {
        id: "inspect_synthetic_av",
        class: "end_to_end",
        scope: "includes input hashing and FFprobe",
        process: $inspect_metrics[0],
        output_bytes: $inspect_output_bytes,
        diagnostic_bytes: $inspect_diagnostic_bytes,
        result: {
          schema_version: $inspection[0].schema_version,
          source_bytes: $inspection[0].source.size_bytes,
          stream_count: ($inspection[0].streams | length)
        }
      },
      {
        id: "plan_synthetic_clip",
        class: "end_to_end",
        scope: "includes input hashing, FFprobe, and FFmpeg capability checks",
        process: $plan_metrics[0],
        output_bytes: $plan_output_bytes,
        diagnostic_bytes: $plan_diagnostic_bytes,
        result: {
          schema_version: $plan[0].schema_version,
          plan_id: $plan[0].id,
          operation: $plan[0].operation.type,
          planned_duration_ms: $plan[0].operation.duration_ms,
          backend_argument_count: ($plan[0].backend.argv | length),
          verification_check_count: ($plan[0].verification_checks | length),
          resource_class: $plan[0].resources.class
        }
      },
      {
        id: "apply_verified_clip",
        class: "end_to_end",
        scope: "CLI process tree including FFmpeg and verification FFprobe",
        process: $apply_metrics[0],
        output_bytes: $apply_output_bytes,
        diagnostic_bytes: $progress_output_bytes,
        receipt_bytes: $receipt_bytes,
        media_output_bytes: $output_media_bytes,
        result: {
          schema_version: $receipt[0].schema_version,
          receipt_id: $receipt[0].id,
          internal_elapsed_ms: $receipt[0].elapsed_ms,
          progress_events: ($progress | length),
          progress_finished: any($progress[]; .state == "finished"),
          verification_passed: $receipt[0].verification.passed,
          checks_passed: all($receipt[0].verification.checks[]; .passed),
          output_sha256: $receipt[0].verification.output.source.sha256,
          publication_method: $receipt[0].publication.method,
          temporary_output_remaining: false
        }
      },
      {
        id: "verify_published_clip",
        class: "end_to_end",
        scope: "includes output hashing, FFprobe, and measurement-based FFmpeg checks",
        process: $verify_metrics[0],
        output_bytes: $verify_output_bytes,
        diagnostic_bytes: $verify_diagnostic_bytes,
        result: {
          schema_version: $verification[0].schema_version,
          passed: $verification[0].passed,
          checks_passed: all($verification[0].checks[]; .passed)
        }
      }
    ],
    derived: {
      max_command_tree_rss_mib:
        ([
          $catalog_metrics[0].max_rss_kib,
          $schema_metrics[0].max_rss_kib,
          $inspect_metrics[0].max_rss_kib,
          $plan_metrics[0].max_rss_kib,
          $apply_metrics[0].max_rss_kib,
          $verify_metrics[0].max_rss_kib
        ] | max | . / 1024),
      contract_max_wall_ms:
        ([
          $catalog_metrics[0].wall_seconds,
          $schema_metrics[0].wall_seconds
        ] | max | . * 1000)
    },
    threshold_status: "observation_only"
  }' >"${result_path}"

jq -e '
  .schema_version == "avpact.benchmark.v1"
  and .fixture.requested_duration_ms == 500
  and .fixture.source_bytes > 0
  and (.fixture.source_sha256 | type == "string" and length == 64)
  and all(
    .measurements[];
    .process.exit_code == 0
      and .process.wall_seconds >= 0
      and .process.max_rss_kib > 0
      and .output_bytes > 0
  )
  and any(
    .measurements[];
    .id == "contract_catalog"
      and .result.schema_version == "avpact.schema-catalog/v0.1"
      and .result.document_count == 8
  )
  and any(
    .measurements[];
    .id == "contract_plan_schema"
      and .result.draft == "https://json-schema.org/draft/2020-12/schema"
      and .result.title == "Plan"
  )
  and any(
    .measurements[];
    .id == "plan_synthetic_clip"
      and .result.operation == "clip"
      and .result.planned_duration_ms == 300
      and .result.backend_argument_count > 0
      and .result.verification_check_count > 0
  )
  and any(
    .measurements[];
    .id == "apply_verified_clip"
      and .result.progress_events > 0
      and .result.progress_finished
      and .result.verification_passed
      and .result.checks_passed
      and (.result.temporary_output_remaining | not)
  )
  and any(
    .measurements[];
    .id == "verify_published_clip"
      and .result.passed
      and .result.checks_passed
  )
' "${result_path}" >/dev/null

printf 'wrote %s\n' "${result_path}"

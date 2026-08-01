#!/usr/bin/env bash

compliance_repository_root() {
    local path_helper_dir path_repository_root

    path_helper_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
    path_repository_root="$(git -C "${path_helper_dir}/.." rev-parse --show-toplevel)"
    realpath -e -- "${path_repository_root}"
}

canonical_compliance_output_dir() {
    if [[ "$#" -ne 2 ]]; then
        echo "error: canonical_compliance_output_dir requires a repository root and output path" >&2
        return 2
    fi

    local path_repository_root="$1"
    local path_requested_output="$2"
    local path_target_root path_requested_path path_canonical_output

    path_target_root="$(realpath -m -- "${path_repository_root}/target")"
    if [[ "${path_requested_output}" == /* ]]; then
        path_requested_path="${path_requested_output}"
    else
        path_requested_path="${path_repository_root}/${path_requested_output}"
    fi
    path_canonical_output="$(realpath -m -- "${path_requested_path}")"

    if [[ "${path_canonical_output}" != "${path_target_root}"/* ]]; then
        echo "error: output path must be below ${path_target_root}: ${path_requested_output}" >&2
        return 1
    fi

    printf '%s\n' "${path_canonical_output}"
}

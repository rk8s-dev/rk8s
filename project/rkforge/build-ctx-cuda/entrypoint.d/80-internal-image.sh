#!/bin/bash
# Copyright (c) 2022-2023 NVIDIA CORPORATION & AFFILIATES. All rights reserved.

internal_only_deprecation_notice() {
    echo
    print_banner_text "*" "PRIVATE IMAGE"
    echo "This image is PRIVATE."
    echo "FOR INTERNAL USE ONLY"
    echo "DO NOT ship to external customers!"
}

[ -n "${NVIDIA_CUDA_INTERNAL}" ] && internal_only_deprecation_notice

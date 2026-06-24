#!/bin/bash
# Copyright (c) 2022-2023 NVIDIA CORPORATION & AFFILIATES. All rights reserved.

show_deprecation_notice() {
    echo
    print_banner_text "*" "DEPRECATION NOTICE!"
    echo "THIS IMAGE IS DEPRECATED and is scheduled for DELETION."
    echo "    https://gitlab.com/nvidia/container-images/cuda/blob/master/doc/support-policy.md"
}

[ -n "${NVIDIA_CUDA_END_OF_LIFE}" ] && show_deprecation_notice

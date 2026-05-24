#!/bin/bash
# Copyright (c) 2022-2023 NVIDIA CORPORATION & AFFILIATES. All rights reserved.

_prodname_uc=$(echo "${NVIDIA_PRODUCT_NAME}" | tr [:lower:] [:upper:] | sed 's/ /_/g' | sed 's/^NVIDIA_//')  # Product name
_compver="${_prodname_uc}_VERSION"        # Upstream component version variable name

echo
[ -n "${!_compver}" ] && echo "${NVIDIA_PRODUCT_NAME} Version ${!_compver}"

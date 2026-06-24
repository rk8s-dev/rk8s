#!/bin/bash
# Copyright (c) 2022-2023 NVIDIA CORPORATION & AFFILIATES. All rights reserved.

_banner_file="${BASH_SOURCE[0]/%.sh/.txt}"

# 10-banner.sh allows itself to be skipped if there exists a
# 10-banner.txt, which will be cat'd next alphabetically
if [[ ! -f "${_banner_file}" && -n "${NVIDIA_PRODUCT_NAME}" ]]; then
  echo
  print_banner_text "=" "${NVIDIA_PRODUCT_NAME}"
fi

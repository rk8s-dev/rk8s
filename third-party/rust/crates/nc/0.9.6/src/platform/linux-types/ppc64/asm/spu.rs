// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `arch/powerpc/include/asm/spu.h`

pub const LS_SIZE: i32 = 256 * 1024;
pub const LS_ADDR_MASK: i32 = LS_SIZE - 1;

pub const MFC_PUT_CMD: i32 = 0x20;
pub const MFC_PUTS_CMD: i32 = 0x28;
pub const MFC_PUTR_CMD: i32 = 0x30;
pub const MFC_PUTF_CMD: i32 = 0x22;
pub const MFC_PUTB_CMD: i32 = 0x21;
pub const MFC_PUTFS_CMD: i32 = 0x2A;
pub const MFC_PUTBS_CMD: i32 = 0x29;
pub const MFC_PUTRF_CMD: i32 = 0x32;
pub const MFC_PUTRB_CMD: i32 = 0x31;
pub const MFC_PUTL_CMD: i32 = 0x24;
pub const MFC_PUTRL_CMD: i32 = 0x34;
pub const MFC_PUTLF_CMD: i32 = 0x26;
pub const MFC_PUTLB_CMD: i32 = 0x25;
pub const MFC_PUTRLF_CMD: i32 = 0x36;
pub const MFC_PUTRLB_CMD: i32 = 0x35;

pub const MFC_GET_CMD: i32 = 0x40;
pub const MFC_GETS_CMD: i32 = 0x48;
pub const MFC_GETF_CMD: i32 = 0x42;
pub const MFC_GETB_CMD: i32 = 0x41;
pub const MFC_GETFS_CMD: i32 = 0x4A;
pub const MFC_GETBS_CMD: i32 = 0x49;
pub const MFC_GETL_CMD: i32 = 0x44;
pub const MFC_GETLF_CMD: i32 = 0x46;
pub const MFC_GETLB_CMD: i32 = 0x45;

pub const MFC_SDCRT_CMD: i32 = 0x80;
pub const MFC_SDCRTST_CMD: i32 = 0x81;
pub const MFC_SDCRZ_CMD: i32 = 0x89;
pub const MFC_SDCRS_CMD: i32 = 0x8D;
pub const MFC_SDCRF_CMD: i32 = 0x8F;

pub const MFC_GETLLAR_CMD: i32 = 0xD0;
pub const MFC_PUTLLC_CMD: i32 = 0xB4;
pub const MFC_PUTLLUC_CMD: i32 = 0xB0;
pub const MFC_PUTQLLUC_CMD: i32 = 0xB8;
pub const MFC_SNDSIG_CMD: i32 = 0xA0;
pub const MFC_SNDSIGB_CMD: i32 = 0xA1;
pub const MFC_SNDSIGF_CMD: i32 = 0xA2;
pub const MFC_BARRIER_CMD: i32 = 0xC0;
pub const MFC_EIEIO_CMD: i32 = 0xC8;
pub const MFC_SYNC_CMD: i32 = 0xCC;

/// 16 bytes
pub const MFC_MIN_DMA_SIZE_SHIFT: i32 = 4;
/// 16384 bytes
pub const MFC_MAX_DMA_SIZE_SHIFT: i32 = 14;
pub const MFC_MIN_DMA_SIZE: i32 = (1 << MFC_MIN_DMA_SIZE_SHIFT);
pub const MFC_MAX_DMA_SIZE: i32 = (1 << MFC_MAX_DMA_SIZE_SHIFT);
pub const MFC_MIN_DMA_SIZE_MASK: i32 = (MFC_MIN_DMA_SIZE - 1);
pub const MFC_MAX_DMA_SIZE_MASK: i32 = (MFC_MAX_DMA_SIZE - 1);
/// 8 bytes
pub const MFC_MIN_DMA_LIST_SIZE: i32 = 0x0008;
/// 16K bytes
pub const MFC_MAX_DMA_LIST_SIZE: i32 = 0x4000;

#[inline]
pub fn MFC_TAGID_TO_TAGMASK(tag_id: i32) -> i32 {
    1 << (tag_id & 0x1F)
}

/// Events for Channels 0-2
pub const MFC_DMA_TAG_STATUS_UPDATE_EVENT: i32 = 0x00000001;
pub const MFC_DMA_TAG_CMD_STALL_NOTIFY_EVENT: i32 = 0x00000002;
pub const MFC_DMA_QUEUE_AVAILABLE_EVENT: i32 = 0x00000008;
pub const MFC_SPU_MAILBOX_WRITTEN_EVENT: i32 = 0x00000010;
pub const MFC_DECREMENTER_EVENT: i32 = 0x00000020;
pub const MFC_PU_INT_MAILBOX_AVAILABLE_EVENT: i32 = 0x00000040;
pub const MFC_PU_MAILBOX_AVAILABLE_EVENT: i32 = 0x00000080;
pub const MFC_SIGNAL_2_EVENT: i32 = 0x00000100;
pub const MFC_SIGNAL_1_EVENT: i32 = 0x00000200;
pub const MFC_LLR_LOST_EVENT: i32 = 0x00000400;
pub const MFC_PRIV_ATTN_EVENT: i32 = 0x00000800;
pub const MFC_MULTI_SRC_EVENT: i32 = 0x00001000;

/// Flag indicating progress during context switch.
pub const SPU_CONTEXT_SWITCH_PENDING: usize = 0;
pub const SPU_CONTEXT_FAULT_PENDING: usize = 1;

pub const SPU_UTIL_USER: i32 = 0;
pub const SPU_UTIL_SYSTEM: i32 = 1;
pub const SPU_UTIL_IOWAIT: i32 = 2;
pub const SPU_UTIL_IDLE_LOADED: i32 = 3;
pub const SPU_UTIL_MA: i32 = 4;

/// return status from spu_run, same as in libspe
/// A DMA alignment error
pub const SPE_EVENT_DMA_ALIGNMENT: i32 = 0x0008;
/// An illegal instruction error
pub const SPE_EVENT_SPE_ERROR: i32 = 0x0010;
/// A DMA segmentation error
pub const SPE_EVENT_SPE_DATA_SEGMENT: i32 = 0x0020;
/// A DMA storage error
pub const SPE_EVENT_SPE_DATA_STORAGE: i32 = 0x0040;
/// Invalid MFC DMA
pub const SPE_EVENT_INVALID_DMA: i32 = 0x0800;

/// Flags for sys_spu_create.
pub const SPU_CREATE_EVENTS_ENABLED: i32 = 0x0001;
pub const SPU_CREATE_GANG: i32 = 0x0002;
pub const SPU_CREATE_NOSCHED: i32 = 0x0004;
pub const SPU_CREATE_ISOLATE: i32 = 0x0008;
pub const SPU_CREATE_AFFINITY_SPU: i32 = 0x0010;
pub const SPU_CREATE_AFFINITY_MEM: i32 = 0x0020;

/// mask of all valid flags
pub const SPU_CREATE_FLAG_ALL: i32 = 0x003f;

pub const DMA_TAGSTATUS_INTR_ANY: u32 = 1;
pub const DMA_TAGSTATUS_INTR_ALL: u32 = 2;
pub const SPU_RUNCNTL_STOP: isize = 0;
pub const SPU_RUNCNTL_RUNNABLE: isize = 1;
pub const SPU_RUNCNTL_ISOLATE: isize = 2;
pub const SPU_STOP_STATUS_SHIFT: i32 = 16;
pub const SPU_STATUS_STOPPED: i32 = 0x0;
pub const SPU_STATUS_RUNNING: i32 = 0x1;
pub const SPU_STATUS_STOPPED_BY_STOP: i32 = 0x2;
pub const SPU_STATUS_STOPPED_BY_HALT: i32 = 0x4;
pub const SPU_STATUS_WAITING_FOR_CHANNEL: i32 = 0x8;
pub const SPU_STATUS_SINGLE_STEP: i32 = 0x10;
pub const SPU_STATUS_INVALID_INSTR: i32 = 0x20;
pub const SPU_STATUS_INVALID_CH: i32 = 0x40;
pub const SPU_STATUS_ISOLATED_STATE: i32 = 0x80;
pub const SPU_STATUS_ISOLATED_LOAD_STATUS: i32 = 0x200;
pub const SPU_STATUS_ISOLATED_EXIT_STATUS: i32 = 0x400;

pub const SLB_INDEX_MASK: isize = 0x7;
pub const SLB_VSID_SUPERVISOR_STATE: u64 = 0x1 << 11;
pub const SLB_VSID_SUPERVISOR_STATE_MASK: u64 = 0x1 << 11;
pub const SLB_VSID_PROBLEM_STATE: u64 = 0x1 << 10;
pub const SLB_VSID_PROBLEM_STATE_MASK: u64 = 0x1 << 10;
pub const SLB_VSID_EXECUTE_SEGMENT: u64 = 0x1 << 9;
pub const SLB_VSID_NO_EXECUTE_SEGMENT: u64 = 0x1 << 9;
pub const SLB_VSID_EXECUTE_SEGMENT_MASK: u64 = 0x1 << 9;
pub const SLB_VSID_4K_PAGE: i32 = 0x0 << 8;
pub const SLB_VSID_LARGE_PAGE: u64 = 0x1 << 8;
pub const SLB_VSID_PAGE_SIZE_MASK: u64 = 0x1 << 8;
pub const SLB_VSID_CLASS_MASK: u64 = 0x1 << 7;
pub const SLB_VSID_VIRTUAL_PAGE_SIZE_MASK: u64 = 0x1 << 6;
pub const MFC_CNTL_RESUME_DMA_QUEUE: u64 = 0 << 0;
pub const MFC_CNTL_SUSPEND_DMA_QUEUE: u64 = 1 << 0;
pub const MFC_CNTL_SUSPEND_DMA_QUEUE_MASK: u64 = 1 << 0;
pub const MFC_CNTL_SUSPEND_MASK: u64 = 1 << 4;
pub const MFC_CNTL_NORMAL_DMA_QUEUE_OPERATION: u64 = 0 << 8;
pub const MFC_CNTL_SUSPEND_IN_PROGRESS: u64 = 1 << 8;
pub const MFC_CNTL_SUSPEND_COMPLETE: u64 = 3 << 8;
pub const MFC_CNTL_SUSPEND_DMA_STATUS_MASK: u64 = 3 << 8;
pub const MFC_CNTL_DMA_QUEUES_EMPTY: u64 = 1 << 14;
pub const MFC_CNTL_DMA_QUEUES_EMPTY_MASK: u64 = 1 << 14;
pub const MFC_CNTL_PURGE_DMA_REQUEST: u64 = 1 << 15;
pub const MFC_CNTL_PURGE_DMA_IN_PROGRESS: u64 = 1 << 24;
pub const MFC_CNTL_PURGE_DMA_COMPLETE: u64 = 3 << 24;
pub const MFC_CNTL_PURGE_DMA_STATUS_MASK: u64 = 3 << 24;
pub const MFC_CNTL_RESTART_DMA_COMMAND: u64 = 1 << 32;
pub const MFC_CNTL_DMA_COMMAND_REISSUE_PENDING: u64 = 1 << 32;
pub const MFC_CNTL_DMA_COMMAND_REISSUE_STATUS_MASK: u64 = 1 << 32;
pub const MFC_CNTL_MFC_PRIVILEGE_STATE: u64 = 2 << 33;
pub const MFC_CNTL_MFC_PROBLEM_STATE: u64 = 3 << 33;
pub const MFC_CNTL_MFC_KEY_PROTECTION_STATE_MASK: u64 = 3 << 33;
pub const MFC_CNTL_DECREMENTER_HALTED: u64 = 1 << 35;
pub const MFC_CNTL_DECREMENTER_RUNNING: u64 = 1 << 40;
pub const MFC_CNTL_DECREMENTER_STATUS_MASK: u64 = 1 << 40;
pub const SPU_PRIVCNTL_MODE_NORMAL: u64 = 0x0 << 0;
pub const SPU_PRIVCNTL_MODE_SINGLE_STEP: u64 = 0x1 << 0;
pub const SPU_PRIVCNTL_MODE_MASK: u64 = 0x1 << 0;
pub const SPU_PRIVCNTL_NO_ATTENTION_EVENT: u64 = 0x0 << 1;
pub const SPU_PRIVCNTL_ATTENTION_EVENT: u64 = 0x1 << 1;
pub const SPU_PRIVCNTL_ATTENTION_EVENT_MASK: u64 = 0x1 << 1;
pub const SPU_PRIVCNT_LOAD_REQUEST_NORMAL: u64 = 0x0 << 2;
pub const SPU_PRIVCNT_LOAD_REQUEST_ENABLE_MASK: u64 = 0x1 << 2;
pub const TAG_STATUS_QUERY_CONDITION_BITS: u64 = 0x3 << 32;
pub const TAG_STATUS_QUERY_MASK_BITS: u64 = 0xffffffff;
pub const SPU_COMMAND_BUFFER_1_LSA_BITS: u64 = 0x7ffff << 32;
pub const SPU_COMMAND_BUFFER_1_EAH_BITS: u64 = 0xffffffff;
pub const SPU_COMMAND_BUFFER_2_EAL_BITS: u64 = 0xffffffff << 32;
pub const SPU_COMMAND_BUFFER_2_TS_BITS: u64 = 0xffff << 16;
pub const SPU_COMMAND_BUFFER_2_TAG_BITS: u64 = 0x3f;
pub const MFC_STATE1_LOCAL_STORAGE_DECODE_MASK: u64 = 0x01;
pub const MFC_STATE1_BUS_TLBIE_MASK: u64 = 0x02;
pub const MFC_STATE1_REAL_MODE_OFFSET_ENABLE_MASK: u64 = 0x04;
pub const MFC_STATE1_PROBLEM_STATE_MASK: u64 = 0x08;
pub const MFC_STATE1_RELOCATE_MASK: u64 = 0x10;
pub const MFC_STATE1_MASTER_RUN_CONTROL_MASK: u64 = 0x20;
pub const MFC_STATE1_TABLE_SEARCH_MASK: u64 = 0x40;
pub const MFC_VERSION_BITS: i32 = 0xffff << 16;
pub const MFC_REVISION_BITS: i32 = 0xffff;

#[inline]
pub fn MFC_GET_VERSION_BITS(vr: i32) -> i32 {
    (vr & MFC_VERSION_BITS) >> 16
}

#[inline]
pub fn MFC_GET_REVISION_BITS(vr: i32) -> i32 {
    vr & MFC_REVISION_BITS
}

pub const SPU_VERSION_BITS: i32 = 0xffff << 16;
pub const SPU_REVISION_BITS: i32 = 0xffff;

#[inline]
pub fn SPU_GET_VERSION_BITS(vr: i32) -> i32 {
    (vr & SPU_VERSION_BITS) >> 16
}

#[inline]
pub fn SPU_GET_REVISION_BITS(vr: i32) -> i32 {
    vr & SPU_REVISION_BITS
}

pub const CLASS0_ENABLE_DMA_ALIGNMENT_INTR: isize = 0x1;
pub const CLASS0_ENABLE_INVALID_DMA_COMMAND_INTR: isize = 0x2;
pub const CLASS0_ENABLE_SPU_ERROR_INTR: isize = 0x4;
pub const CLASS0_ENABLE_MFC_FIR_INTR: isize = 0x8;
pub const CLASS1_ENABLE_SEGMENT_FAULT_INTR: isize = 0x1;
pub const CLASS1_ENABLE_STORAGE_FAULT_INTR: isize = 0x2;
pub const CLASS1_ENABLE_LS_COMPARE_SUSPEND_ON_GET_INTR: isize = 0x4;
pub const CLASS1_ENABLE_LS_COMPARE_SUSPEND_ON_PUT_INTR: isize = 0x8;
pub const CLASS2_ENABLE_MAILBOX_INTR: isize = 0x1;
pub const CLASS2_ENABLE_SPU_STOP_INTR: isize = 0x2;
pub const CLASS2_ENABLE_SPU_HALT_INTR: isize = 0x4;
pub const CLASS2_ENABLE_SPU_DMA_TAG_GROUP_COMPLETE_INTR: isize = 0x8;
pub const CLASS2_ENABLE_MAILBOX_THRESHOLD_INTR: isize = 0x10;
pub const CLASS0_DMA_ALIGNMENT_INTR: isize = 0x1;
pub const CLASS0_INVALID_DMA_COMMAND_INTR: isize = 0x2;
pub const CLASS0_SPU_ERROR_INTR: isize = 0x4;
pub const CLASS0_INTR_MASK: isize = 0x7;
pub const CLASS1_SEGMENT_FAULT_INTR: isize = 0x1;
pub const CLASS1_STORAGE_FAULT_INTR: isize = 0x2;
pub const CLASS1_LS_COMPARE_SUSPEND_ON_GET_INTR: isize = 0x4;
pub const CLASS1_LS_COMPARE_SUSPEND_ON_PUT_INTR: isize = 0x8;
pub const CLASS1_INTR_MASK: isize = 0xf;
pub const CLASS2_MAILBOX_INTR: isize = 0x1;
pub const CLASS2_SPU_STOP_INTR: isize = 0x2;
pub const CLASS2_SPU_HALT_INTR: isize = 0x4;
pub const CLASS2_SPU_DMA_TAG_GROUP_COMPLETE_INTR: isize = 0x8;
pub const CLASS2_MAILBOX_THRESHOLD_INTR: isize = 0x10;
pub const CLASS2_INTR_MASK: isize = 0x1f;
pub const mfc_atomic_flush_enable: isize = 0x1;
pub const smf_sbi_mask_lsb: i32 = 56;
pub const smf_sbi_shift: i32 = 63 - smf_sbi_mask_lsb;
pub const smf_sbi_mask: i64 = 0x301 << smf_sbi_shift;
pub const smf_sbi_bus0_bits: i64 = 0x001 << smf_sbi_shift;
pub const smf_sbi_bus2_bits: i64 = 0x100 << smf_sbi_shift;
pub const smf_sbi2_bus0_bits: i64 = 0x201 << smf_sbi_shift;
pub const smf_sbi2_bus2_bits: i64 = 0x300 << smf_sbi_shift;
pub const smf_ato_mask_lsb: i32 = 35;
pub const smf_ato_shift: i32 = 63 - smf_ato_mask_lsb;
pub const smf_ato_mask: i64 = 0x3 << smf_ato_shift;
pub const smf_ato_bus0_bits: i64 = 0x2 << smf_ato_shift;
pub const smf_ato_bus2_bits: i64 = 0x1 << smf_ato_shift;
pub const PAGE_SIZE_MASK: u64 = 0xf000000000000000;
pub const PAGE_SIZE_16MB_64KB: u64 = 0x2000000000000000;
pub const MFC_ACCR_EA_ACCESS_GET: i32 = 1 << 0;
pub const MFC_ACCR_EA_ACCESS_PUT: i32 = 1 << 1;
pub const MFC_ACCR_LS_ACCESS_GET: i32 = 1 << 3;
pub const MFC_ACCR_LS_ACCESS_PUT: i32 = 1 << 4;
pub const MFC_DSISR_PTE_NOT_FOUND: i32 = 1 << 30;
pub const MFC_DSISR_ACCESS_DENIED: i32 = 1 << 27;
pub const MFC_DSISR_ATOMIC: i32 = 1 << 26;
pub const MFC_DSISR_ACCESS_PUT: i32 = 1 << 25;
pub const MFC_DSISR_ADDR_MATCH: i32 = 1 << 22;
pub const MFC_DSISR_LS: i32 = 1 << 17;
pub const MFC_DSISR_L: i32 = 1 << 16;
pub const MFC_DSISR_ADDRESS_OVERFLOW: i32 = 1 << 0;
pub const MFC_DSIR_Q: i32 = 1 << 31;
pub const MFC_DSIR_SPU_QUEUE: i32 = MFC_DSIR_Q;
pub const MFC_LSACR_COMPARE_MASK: u64 = !0 << 32;
pub const MFC_LSACR_COMPARE_ADDR: u64 = !0 >> 32;
pub const MFC_LSCRR_Q: i32 = 1 << 31;
pub const MFC_LSCRR_SPU_QUEUE: i32 = MFC_LSCRR_Q;
pub const MFC_LSCRR_QI_SHIFT: i32 = 32;
pub const MFC_LSCRR_QI_MASK: u64 = !0 << MFC_LSCRR_QI_SHIFT;
pub const MFC_TCLASS_ID_ENABLE: isize = 1 << 0;
pub const MFC_TCLASS_SLOT2_ENABLE: isize = 1 << 5;
pub const MFC_TCLASS_SLOT1_ENABLE: isize = 1 << 6;
pub const MFC_TCLASS_SLOT0_ENABLE: isize = 1 << 7;
pub const MFC_TCLASS_QUOTA_2_SHIFT: isize = 8;
pub const MFC_TCLASS_QUOTA_1_SHIFT: isize = 16;
pub const MFC_TCLASS_QUOTA_0_SHIFT: isize = 24;
pub const MFC_TCLASS_QUOTA_2_MASK: isize = 0x1F << MFC_TCLASS_QUOTA_2_SHIFT;
pub const MFC_TCLASS_QUOTA_1_MASK: isize = 0x1F << MFC_TCLASS_QUOTA_1_SHIFT;
pub const MFC_TCLASS_QUOTA_0_MASK: isize = 0x1F << MFC_TCLASS_QUOTA_0_SHIFT;
pub const mfc_dma1_mask_lsb: i32 = 41;
pub const mfc_dma1_shift: i32 = 63 - mfc_dma1_mask_lsb;
pub const mfc_dma1_mask: i64 = 0x3 << mfc_dma1_shift;
pub const mfc_dma1_bits: i64 = 0x1 << mfc_dma1_shift;
pub const mfc_dma2_mask_lsb: i32 = 43;
pub const mfc_dma2_shift: i32 = 63 - mfc_dma2_mask_lsb;
pub const mfc_dma2_mask: i64 = 0x3 << mfc_dma2_shift;
pub const mfc_dma2_bits: i64 = 0x1 << mfc_dma2_shift;
pub const smm_sig_mask_lsb: i32 = 12;
pub const smm_sig_shift: i32 = 63 - smm_sig_mask_lsb;
pub const smm_sig_mask: i64 = 0x3 << smm_sig_shift;
pub const smm_sig_bus0_bits: i64 = 0x2 << smm_sig_shift;
pub const smm_sig_bus2_bits: i64 = 0x1 << smm_sig_shift;
pub const MFC_CER_Q: u64 = 1 << 31;
pub const MFC_CER_SPU_QUEUE: u64 = MFC_CER_Q;
pub const SPU_ECC_CNTL_E: u64 = 1 << 0;
pub const SPU_ECC_CNTL_ENABLE: u64 = SPU_ECC_CNTL_E;
pub const SPU_ECC_CNTL_DISABLE: u64 = !SPU_ECC_CNTL_E & 1;
pub const SPU_ECC_CNTL_S: u64 = 1 << 1;
pub const SPU_ECC_STOP_AFTER_ERROR: u64 = SPU_ECC_CNTL_S;
pub const SPU_ECC_CONTINUE_AFTER_ERROR: u64 = !SPU_ECC_CNTL_S & 2;
pub const SPU_ECC_CNTL_B: u64 = 1 << 2;
pub const SPU_ECC_BACKGROUND_ENABLE: u64 = SPU_ECC_CNTL_B;
pub const SPU_ECC_BACKGROUND_DISABLE: u64 = !SPU_ECC_CNTL_B & 4;
pub const SPU_ECC_CNTL_I_SHIFT: u64 = 3;
pub const SPU_ECC_CNTL_I_MASK: u64 = 3 << SPU_ECC_CNTL_I_SHIFT;
pub const SPU_ECC_WRITE_ALWAYS: u64 = !SPU_ECC_CNTL_I & 12;
pub const SPU_ECC_WRITE_CORRECTABLE: u64 = 1 << SPU_ECC_CNTL_I_SHIFT;
pub const SPU_ECC_WRITE_UNCORRECTABLE: u64 = 3 << SPU_ECC_CNTL_I_SHIFT;
pub const SPU_ECC_CNTL_D: u64 = 1 << 5;
pub const SPU_ECC_DETECTION_ENABLE: u64 = SPU_ECC_CNTL_D;
pub const SPU_ECC_DETECTION_DISABLE: u64 = !SPU_ECC_CNTL_D & 32;
pub const SPU_ECC_CORRECTED_ERROR: u64 = 1 << 0;
pub const SPU_ECC_UNCORRECTED_ERROR: u64 = 1 << 1;
pub const SPU_ECC_SCRUB_COMPLETE: u64 = 1 << 2;
pub const SPU_ECC_SCRUB_IN_PROGRESS: u64 = 1 << 3;
pub const SPU_ECC_INSTRUCTION_ERROR: u64 = 1 << 4;
pub const SPU_ECC_DATA_ERROR: u64 = 1 << 5;
pub const SPU_ECC_DMA_ERROR: u64 = 1 << 6;
pub const SPU_ECC_STATUS_CNT_MASK: u64 = 256 << 8;
pub const SPU_ERR_ILLEGAL_INSTR: u64 = 1 << 0;
pub const SPU_ERR_ILLEGAL_CHANNEL: u64 = 1 << 1;
pub const spu_trace_sel_mask: i64 = 0x1f1f;
pub const spu_trace_sel_bus0_bits: i64 = 0x1000;
pub const spu_trace_sel_bus2_bits: i64 = 0x0010;

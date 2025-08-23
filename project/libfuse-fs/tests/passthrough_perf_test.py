#!/usr/bin/env python3
import os
import time
import shutil
import random
import string
import statistics
import argparse
import json
import platform
import subprocess
from multiprocessing import Pool, cpu_count
from functools import partial
from datetime import datetime

def generate_random_content(size):
    """Generate random content of specified size"""
    return ''.join(random.choices(string.ascii_letters + string.digits, k=size)).encode()

def sequential_write_test(test_dir, file_size, file_count):
    """Sequential write test"""
    content = generate_random_content(file_size)
    start_time = time.time()
    
    for i in range(file_count):
        file_path = os.path.join(test_dir, f"seq_write_{i}.dat")
        with open(file_path, 'wb') as f:
            f.write(content)
    
    elapsed = time.time() - start_time
    total_size = file_size * file_count
    speed = (total_size / 1024 / 1024) / elapsed
    return speed

def sequential_read_test(test_dir, file_count):
    """Sequential read test"""
    file_list = [os.path.join(test_dir, f"seq_write_{i}.dat") for i in range(file_count)]
    start_time = time.time()
    
    for file_path in file_list:
        with open(file_path, 'rb') as f:
            _ = f.read()
    
    elapsed = time.time() - start_time
    total_size = os.path.getsize(file_list[0]) * file_count
    speed = (total_size / 1024 / 1024) / elapsed
    return speed

def random_write_test(test_dir, file_size, file_count):
    """Random write test"""
    content = generate_random_content(file_size)
    start_time = time.time()
    
    for i in range(file_count):
        file_path = os.path.join(test_dir, f"rand_write_{i}.dat")
        with open(file_path, 'wb') as f:
            for _ in range(0, file_size, 4096):  # 4KB blocks
                offset = random.randint(0, file_size-4096)
                f.seek(offset)
                f.write(content[offset:offset+4096])
    
    elapsed = time.time() - start_time
    total_size = file_size * file_count
    speed = (total_size / 1024 / 1024) / elapsed
    return speed

def random_read_test(test_dir, file_count):
    """Random read test"""
    file_list = [os.path.join(test_dir, f"rand_write_{i}.dat") for i in range(file_count)]
    file_size = os.path.getsize(file_list[0])
    start_time = time.time()
    
    for file_path in file_list:
        with open(file_path, 'rb') as f:
            for _ in range(100):  # Random read 100 times per file
                offset = random.randint(0, file_size-4096)
                f.seek(offset)
                _ = f.read(4096)
    
    elapsed = time.time() - start_time
    total_size = 4096 * 100 * file_count
    speed = (total_size / 1024 / 1024) / elapsed
    return speed

def create_small_files(test_dir, file_size, file_count):
    """Create many small files test"""
    small_dir = os.path.join(test_dir, "small_files")
    os.makedirs(small_dir, exist_ok=True)
    
    content = generate_random_content(file_size)
    start_time = time.time()
    
    for i in range(file_count):
        file_path = os.path.join(small_dir, f"small_{i}.dat")
        with open(file_path, 'wb') as f:
            f.write(content)
    
    elapsed = time.time() - start_time
    ops = file_count / elapsed
    return ops

def stat_small_files(test_dir, file_count):
    """Small files stat test"""
    small_dir = os.path.join(test_dir, "small_files")
    file_list = [os.path.join(small_dir, f"small_{i}.dat") for i in range(file_count)]
    start_time = time.time()
    
    for file_path in file_list:
        _ = os.stat(file_path)
    
    elapsed = time.time() - start_time
    ops = file_count / elapsed
    return ops

def delete_small_files(test_dir, file_count):
    """Small files delete test"""
    small_dir = os.path.join(test_dir, "small_files")
    file_list = [os.path.join(small_dir, f"small_{i}.dat") for i in range(file_count)]
    start_time = time.time()
    
    for file_path in file_list:
        os.unlink(file_path)
    
    elapsed = time.time() - start_time
    ops = file_count / elapsed
    return ops

def metadata_operations(test_dir, ops_count):
    """Metadata operations test"""
    meta_dir = os.path.join(test_dir, "metadata_test")
    os.makedirs(meta_dir, exist_ok=True)
    
    start_time = time.time()
    
    for i in range(ops_count):
        dir_path = os.path.join(meta_dir, f"dir_{i}")
        os.mkdir(dir_path)
        os.rename(dir_path, dir_path + "_renamed")
        os.rmdir(dir_path + "_renamed")
    
    elapsed = time.time() - start_time
    ops = ops_count * 3 / elapsed  # 3 operations per loop
    return ops

def get_system_info():
    """Get system information"""
    info = {
        "platform": platform.platform(),
        "processor": platform.processor(),
        "python_version": platform.python_version(),
        "hostname": platform.node(),
        "timestamp": datetime.now().isoformat(),
    }
    
    # Get CPU information
    try:
        with open('/proc/cpuinfo', 'r') as f:
            cpuinfo = f.read()
            # Find CPU model
            for line in cpuinfo.split('\n'):
                if 'model name' in line:
                    info['cpu_model'] = line.split(':')[1].strip()
                    break
    except:
        info['cpu_model'] = "Unknown"
    
    # Get memory information
    try:
        with open('/proc/meminfo', 'r') as f:
            meminfo = f.read()
            # Find total memory
            for line in meminfo.split('\n'):
                if 'MemTotal' in line:
                    info['total_memory'] = line.split(':')[1].strip()
                    break
    except:
        info['total_memory'] = "Unknown"
    
    return info

def write_file_helper(args):
    """Concurrent write helper function"""
    test_dir, i, content = args
    file_path = os.path.join(test_dir, f"concurrent_write_{i}.dat")
    with open(file_path, 'wb') as f:
        f.write(content)
    return True

def concurrent_write_test(test_dir, file_size, file_count, num_processes=None):
    """Concurrent write test"""
    if num_processes is None:
        num_processes = cpu_count()
    
    content = generate_random_content(file_size)
    args_list = [(test_dir, i, content) for i in range(file_count)]
    
    start_time = time.time()
    with Pool(num_processes) as pool:
        pool.map(write_file_helper, args_list)
    elapsed = time.time() - start_time
    
    total_size = file_size * file_count
    speed = (total_size / 1024 / 1024) / elapsed
    return speed

def read_file_helper(test_dir, i):
    """Concurrent read helper function"""
    file_path = os.path.join(test_dir, f"concurrent_write_{i}.dat")
    with open(file_path, 'rb') as f:
        _ = f.read()
    return True

def concurrent_read_test(test_dir, file_count, num_processes=None):
    """Concurrent read test"""
    if num_processes is None:
        num_processes = cpu_count()
    
    # Use partial to fix test_dir parameter
    read_file_partial = partial(read_file_helper, test_dir)
    
    start_time = time.time()
    with Pool(num_processes) as pool:
        pool.map(read_file_partial, range(file_count))
    elapsed = time.time() - start_time
    
    file_size = os.path.getsize(os.path.join(test_dir, "concurrent_write_0.dat"))
    total_size = file_size * file_count
    speed = (total_size / 1024 / 1024) / elapsed
    return speed

def symlink_test(test_dir, file_count):
    """Symbolic link test"""
    # Create some files first
    os.makedirs(os.path.join(test_dir, "symlink_source"), exist_ok=True)
    os.makedirs(os.path.join(test_dir, "symlink_target"), exist_ok=True)
    
    # Create source files
    for i in range(file_count):
        file_path = os.path.join(test_dir, "symlink_source", f"source_{i}.dat")
        with open(file_path, 'wb') as f:
            f.write(generate_random_content(1024))  # 1KB file
    
    start_time = time.time()
    # Create symbolic links
    for i in range(file_count):
        source_path = os.path.join(test_dir, "symlink_source", f"source_{i}.dat")
        target_path = os.path.join(test_dir, "symlink_target", f"link_{i}.dat")
        os.symlink(source_path, target_path)
    elapsed = time.time() - start_time
    
    ops = file_count / elapsed
    return ops

def hardlink_test(test_dir, file_count):
    """Hard link test"""
    # Create a source file first
    source_file = os.path.join(test_dir, "hardlink_source.dat")
    with open(source_file, 'wb') as f:
        f.write(generate_random_content(1024))  # 1KB file
    
    start_time = time.time()
    # Create hard links
    for i in range(file_count):
        target_path = os.path.join(test_dir, f"hardlink_{i}.dat")
        os.link(source_file, target_path)
    elapsed = time.time() - start_time
    
    ops = file_count / elapsed
    return ops

def cleanup(test_dir):
    """Clean up test files"""
    if os.path.exists(test_dir):
        shutil.rmtree(test_dir)

def run_single_test_round(test_dir, file_size, file_count, small_file_size, small_file_count, ops_count, round_num):
    """Run a single test round"""
    print(f"\n=== Starting Round {round_num} ===")
    os.makedirs(test_dir, exist_ok=True)
    
    results = {}
    
    # Large file sequential read/write tests
    results['seq_write'] = sequential_write_test(test_dir, file_size, file_count)
    print(f"Sequential Write Speed: {results['seq_write']:.2f} MB/s")
    
    results['seq_read'] = sequential_read_test(test_dir, file_count)
    print(f"Sequential Read Speed: {results['seq_read']:.2f} MB/s")
    
    # Large file random read/write tests
    results['rand_write'] = random_write_test(test_dir, file_size, file_count)
    print(f"Random Write Speed: {results['rand_write']:.2f} MB/s")
    
    results['rand_read'] = random_read_test(test_dir, file_count)
    print(f"Random Read Speed: {results['rand_read']:.2f} MB/s")
    
    # Concurrent read/write tests
    results['concurrent_write'] = concurrent_write_test(test_dir, file_size, file_count)
    print(f"Concurrent Write Speed: {results['concurrent_write']:.2f} MB/s")
    
    results['concurrent_read'] = concurrent_read_test(test_dir, file_count)
    print(f"Concurrent Read Speed: {results['concurrent_read']:.2f} MB/s")
    
    # Small files tests
    results['small_create'] = create_small_files(test_dir, small_file_size, small_file_count)
    print(f"Small Files Creation Speed: {results['small_create']:.2f} files/sec")
    
    results['small_stat'] = stat_small_files(test_dir, small_file_count)
    print(f"Small Files Stat Speed: {results['small_stat']:.2f} ops/sec")
    
    results['small_delete'] = delete_small_files(test_dir, small_file_count)
    print(f"Small Files Deletion Speed: {results['small_delete']:.2f} files/sec")
    
    # Link tests
    results['symlink_ops'] = symlink_test(test_dir, small_file_count // 10)  # Reduce link test count to avoid taking too long
    print(f"Symbolic Link Speed: {results['symlink_ops']:.2f} ops/sec")
    
    results['hardlink_ops'] = hardlink_test(test_dir, small_file_count // 10)
    print(f"Hard Link Speed: {results['hardlink_ops']:.2f} ops/sec")
    
    # Metadata operations test
    results['metadata_ops'] = metadata_operations(test_dir, ops_count)
    print(f"Metadata Operations Speed: {results['metadata_ops']:.2f} ops/sec")
    
    # Clean up test files
    cleanup(test_dir)
    
    print(f"=== Round {round_num} Completed ===")
    return results

def calculate_statistics(all_results):
    """Calculate statistics for multiple test rounds"""
    stats = {}
    for test_name in all_results[0].keys():
        values = [round_result[test_name] for round_result in all_results]
        stats[test_name] = {
            'avg': statistics.mean(values),
            'min': min(values),
            'max': max(values),
            'stdev': statistics.stdev(values) if len(values) > 1 else 0,
            'values': values
        }
    return stats

def print_final_report(stats, rounds, json_output=None):
    """Print final test report"""
    print("\n=== Final Test Report ===")
    print(f"Number of test rounds: {rounds}")
    print("\n{:<20} {:<10} {:<10} {:<10} {:<10} {:<10}".format(
        "Test Item", "Average", "Min", "Max", "Std Dev", "Unit"))
    
    # Define which tests use MB/s unit
    mbps_tests = ['seq_write', 'seq_read', 'rand_write', 'rand_read', 'concurrent_write', 'concurrent_read']
    
    for test_name, data in stats.items():
        unit = "MB/s" if test_name in mbps_tests else "ops/sec"
        print("{:<20} {:<10.2f} {:<10.2f} {:<10.2f} {:<10.2f} {:<10}".format(
            test_name.replace('_', ' ').title(),
            data['avg'],
            data['min'],
            data['max'],
            data['stdev'],
            unit
        ))
    
    # Save results to JSON file if specified
    if json_output:
        result = {
            "system_info": get_system_info(),
            "test_config": {
                "rounds": rounds
            },
            "results": stats
        }
        with open(json_output, 'w') as f:
            json.dump(result, f, indent=2, ensure_ascii=False)
        print(f"\nResults saved to {json_output}")

def run_tests(test_dir, file_size=10*1024*1024, file_count=10, 
             small_file_size=1024, small_file_count=1000, ops_count=1000, rounds=3, json_output=None):
    """Run multiple test rounds and calculate averages"""
    print(f"Starting filesystem performance test, test directory: {test_dir}")
    print(f"Number of test rounds: {rounds}")
    
    all_results = []
    
    for i in range(rounds):
        round_result = run_single_test_round(
            test_dir, file_size, file_count, 
            small_file_size, small_file_count, ops_count, i+1
        )
        all_results.append(round_result)
    
    stats = calculate_statistics(all_results)
    print_final_report(stats, rounds, json_output)
    
    return stats

def main():
    parser = argparse.ArgumentParser(description='Filesystem performance testing tool (multi-round testing)')
    parser.add_argument('test_dir', help='Test directory path')
    parser.add_argument('--file-size', type=int, default=100*1024*1024, 
                        help='Large file size (bytes), default 100MB')
    parser.add_argument('--file-count', type=int, default=20, 
                        help='Number of large files, default 20')
    parser.add_argument('--small-file-size', type=int, default=1024, 
                        help='Small file size (bytes), default 1KB')
    parser.add_argument('--small-file-count', type=int, default=1000, 
                        help='Number of small files, default 1000')
    parser.add_argument('--ops-count', type=int, default=1000, 
                        help='Number of metadata operations, default 1000')
    parser.add_argument('--rounds', type=int, default=3, 
                        help='Number of test rounds, default 3')
    parser.add_argument('--no-cleanup', action='store_true', 
                        help='Do not clean up files after testing')
    parser.add_argument('--json-output', type=str, 
                        help='Save results to specified JSON file')
    
    args = parser.parse_args()
    
    if args.no_cleanup:
        global cleanup
        cleanup = lambda x: None
    
    stats = run_tests(
        args.test_dir,
        file_size=args.file_size,
        file_count=args.file_count,
        small_file_size=args.small_file_size,
        small_file_count=args.small_file_count,
        ops_count=args.ops_count,
        rounds=args.rounds,
        json_output=args.json_output
    )

if __name__ == "__main__":
    main()

// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

/// This code is rewritten of tee(2) example code.
/// ```C
/// #define _GNU_SOURCE
/// #include <errno.h>
/// #include <fcntl.h>
/// #include <limits.h>
/// #include <stdio.h>
/// #include <stdlib.h>
/// #include <unistd.h>
///
/// int main(int argc, char *argv[]) {
///   int fd;
///   int len, slen;
///
///   if (argc != 2) {
///     fprintf(stderr, "Usage: %s <file>\n", argv[0]);
///     exit(EXIT_FAILURE);
///   }
///
///   fd = open(argv[1], O_WRONLY | O_CREAT | O_TRUNC, 0644);
///   if (fd == -1) {
///     perror("open");
///     exit(EXIT_FAILURE);
///   }
///
///   do {
///     /*
///      * tee stdin to stdout.
///      */
///     len = tee(STDIN_FILENO, STDOUT_FILENO, INT_MAX, SPLICE_F_NONBLOCK);
///
///     if (len < 0) {
///       if (errno == EAGAIN)
///         continue;
///       perror("tee");
///       exit(EXIT_FAILURE);
///     } else if (len == 0)
///       break;
///
///     /*
///      * Consume stdin by splicing it to a file.
///      */
///     while (len > 0) {
///       slen = splice(STDIN_FILENO, NULL, fd, NULL, len, SPLICE_F_MOVE);
///       if (slen < 0) {
///         perror("splice");
///         break;
///       }
///       len -= slen;
///     }
///   } while (1);
///
///   close(fd);
///   exit(EXIT_SUCCESS);
/// }
/// ```
fn main() {
    let output_file = "/tmp/nc-splice";
    let ret = unsafe {
        nc::openat(
            nc::AT_FDCWD,
            output_file,
            nc::O_WRONLY | nc::O_CREAT | nc::O_TRUNC,
            0o644,
        )
    };
    assert!(ret.is_ok());
    let fd = ret.unwrap();

    // Tee stdin to stdout
    loop {
        let stdin_fileno = 0;
        let stdout_fileno = 1;
        let ret = unsafe {
            nc::tee(
                stdin_fileno,
                stdout_fileno,
                usize::MAX,
                nc::SPLICE_F_NONBLOCK,
            )
        };
        let mut tee_len = match ret {
            Ok(0) => break,
            Err(nc::EAGAIN) => continue,
            Err(errno) => {
                eprintln!("tee error: {}", nc::strerror(errno));
                unsafe { nc::exit(1) };
            }
            Ok(len) => len,
        };

        // Consume stdin by splicing it to a file.
        while tee_len > 0 {
            let ret = unsafe {
                nc::splice(
                    stdin_fileno,
                    None,
                    fd,
                    None,
                    tee_len as usize,
                    nc::SPLICE_F_MOVE,
                )
            };
            match ret {
                Err(errno) => {
                    eprintln!("splice error: {}", nc::strerror(errno));
                    break;
                }
                Ok(len) => tee_len -= len,
            }
        }
    }

    let ret = unsafe { nc::close(fd) };
    assert!(ret.is_ok());
}

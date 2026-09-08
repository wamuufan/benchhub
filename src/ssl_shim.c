#define _GNU_SOURCE
#include <dlfcn.h>
#include <string.h>
#include <fcntl.h>
#include <stdarg.h>
#include <stdio.h>
#include <unistd.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <sys/mman.h>

#ifndef O_TMPFILE
#define O_TMPFILE (__O_TMPFILE | O_DIRECTORY)
#endif

static const char* get_real_ca_path(void) {
    static const char* candidate_paths[] = {
        "/etc/ssl/ca-bundle.pem",
        "/var/lib/ca-certificates/ca-bundle.pem",
        "/etc/pki/tls/certs/ca-bundle.crt",
        "/etc/pki/tls/cert.pem",
        "/etc/ssl/certs/ca-certificates.crt",
        NULL
    };
    for (int i = 0; candidate_paths[i] != NULL; i++) {
        if (access(candidate_paths[i], R_OK) == 0) {
            return candidate_paths[i];
        }
    }
    return "/etc/ssl/ca-bundle.pem";
}

static inline int should_redirect(const char* path) {
    if (!path) return 0;
    return (strcmp(path, "/etc/ssl/certs/ca-certificates.crt") == 0 ||
            strcmp(path, "/etc/ssl/cert.pem") == 0);
}

typedef int (*orig_open_f_type)(const char *pathname, int flags, ...);
typedef int (*orig_open64_f_type)(const char *pathname, int flags, ...);
typedef int (*orig_openat_f_type)(int dirfd, const char *pathname, int flags, ...);
typedef FILE* (*orig_fopen_f_type)(const char *pathname, const char *mode);
typedef FILE* (*orig_fopen64_f_type)(const char *pathname, const char *mode);
typedef int (*orig_access_f_type)(const char *pathname, int mode);
typedef int (*orig_faccessat_f_type)(int dirfd, const char *pathname, int mode, int flags);
typedef int (*orig_stat_f_type)(const char *pathname, struct stat *statbuf);
typedef int (*orig_stat64_f_type)(const char *pathname, struct stat64 *statbuf);
typedef int (*orig_lstat_f_type)(const char *pathname, struct stat *statbuf);
typedef int (*orig_lstat64_f_type)(const char *pathname, struct stat64 *statbuf);
typedef int (*orig_fstatat_f_type)(int dirfd, const char *pathname, struct stat *statbuf, int flags);

int open(const char *pathname, int flags, ...) {
    static orig_open_f_type orig_open = NULL;
    if (!orig_open) orig_open = (orig_open_f_type)dlsym(RTLD_NEXT, "open");
    const char *target = should_redirect(pathname) ? get_real_ca_path() : pathname;
    if ((flags & O_CREAT) || ((flags & O_TMPFILE) == O_TMPFILE)) {
        va_list args;
        va_start(args, flags);
        mode_t mode = va_arg(args, mode_t);
        va_end(args);
        return orig_open(target, flags, mode);
    }
    return orig_open(target, flags);
}

int open64(const char *pathname, int flags, ...) {
    static orig_open64_f_type orig_open64 = NULL;
    if (!orig_open64) orig_open64 = (orig_open64_f_type)dlsym(RTLD_NEXT, "open64");
    const char *target = should_redirect(pathname) ? get_real_ca_path() : pathname;
    if ((flags & O_CREAT) || ((flags & O_TMPFILE) == O_TMPFILE)) {
        va_list args;
        va_start(args, flags);
        mode_t mode = va_arg(args, mode_t);
        va_end(args);
        return orig_open64(target, flags, mode);
    }
    return orig_open64(target, flags);
}

int openat(int dirfd, const char *pathname, int flags, ...) {
    static orig_openat_f_type orig_openat = NULL;
    if (!orig_openat) orig_openat = (orig_openat_f_type)dlsym(RTLD_NEXT, "openat");
    const char *target = should_redirect(pathname) ? get_real_ca_path() : pathname;
    if ((flags & O_CREAT) || ((flags & O_TMPFILE) == O_TMPFILE)) {
        va_list args;
        va_start(args, flags);
        mode_t mode = va_arg(args, mode_t);
        va_end(args);
        return orig_openat(dirfd, target, flags, mode);
    }
    return orig_openat(dirfd, target, flags);
}

FILE *fopen(const char *pathname, const char *mode) {
    static orig_fopen_f_type orig_fopen = NULL;
    if (!orig_fopen) orig_fopen = (orig_fopen_f_type)dlsym(RTLD_NEXT, "fopen");
    return orig_fopen(should_redirect(pathname) ? get_real_ca_path() : pathname, mode);
}

FILE *fopen64(const char *pathname, const char *mode) {
    static orig_fopen64_f_type orig_fopen64 = NULL;
    if (!orig_fopen64) orig_fopen64 = (orig_fopen64_f_type)dlsym(RTLD_NEXT, "fopen64");
    return orig_fopen64(should_redirect(pathname) ? get_real_ca_path() : pathname, mode);
}

int access(const char *pathname, int mode) {
    static orig_access_f_type orig_access = NULL;
    if (!orig_access) orig_access = (orig_access_f_type)dlsym(RTLD_NEXT, "access");
    return orig_access(should_redirect(pathname) ? get_real_ca_path() : pathname, mode);
}

int faccessat(int dirfd, const char *pathname, int mode, int flags) {
    static orig_faccessat_f_type orig_faccessat = NULL;
    if (!orig_faccessat) orig_faccessat = (orig_faccessat_f_type)dlsym(RTLD_NEXT, "faccessat");
    return orig_faccessat(dirfd, should_redirect(pathname) ? get_real_ca_path() : pathname, mode, flags);
}

int stat(const char *pathname, struct stat *statbuf) {
    static orig_stat_f_type orig_stat = NULL;
    if (!orig_stat) orig_stat = (orig_stat_f_type)dlsym(RTLD_NEXT, "stat");
    return orig_stat(should_redirect(pathname) ? get_real_ca_path() : pathname, statbuf);
}

int stat64(const char *pathname, struct stat64 *statbuf) {
    static orig_stat64_f_type orig_stat64 = NULL;
    if (!orig_stat64) orig_stat64 = (orig_stat64_f_type)dlsym(RTLD_NEXT, "stat64");
    return orig_stat64(should_redirect(pathname) ? get_real_ca_path() : pathname, statbuf);
}

int lstat(const char *pathname, struct stat *statbuf) {
    static orig_lstat_f_type orig_lstat = NULL;
    if (!orig_lstat) orig_lstat = (orig_lstat_f_type)dlsym(RTLD_NEXT, "lstat");
    return orig_lstat(should_redirect(pathname) ? get_real_ca_path() : pathname, statbuf);
}

int lstat64(const char *pathname, struct stat64 *statbuf) {
    static orig_lstat64_f_type orig_lstat64 = NULL;
    if (!orig_lstat64) orig_lstat64 = (orig_lstat64_f_type)dlsym(RTLD_NEXT, "lstat64");
    return orig_lstat64(should_redirect(pathname) ? get_real_ca_path() : pathname, statbuf);
}

int fstatat(int dirfd, const char *pathname, struct stat *statbuf, int flags) {
    static orig_fstatat_f_type orig_fstatat = NULL;
    if (!orig_fstatat) orig_fstatat = (orig_fstatat_f_type)dlsym(RTLD_NEXT, "fstatat");
    return orig_fstatat(dirfd, should_redirect(pathname) ? get_real_ca_path() : pathname, statbuf, flags);
}


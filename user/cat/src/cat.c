/*
 * V7 UNIX cat.c program body, adapted to nk by replacing stdio/libc with a
 * tiny syscall runtime. Source reference:
 * https://www.tuhs.org/cgi-bin/utree.pl?file=V7/usr/src/cmd/cat.c
 */

#define BUFSIZ 512
#define EOF (-1)
#define NULL ((void *)0)
#define S_IFMT 0170000
#define S_IFCHR 0020000
#define S_IFBLK 0060000

typedef unsigned long size_t;

struct stat {
    int st_dev;
    int st_ino;
    int st_mode;
};

typedef struct {
    int fd;
    unsigned char buf[BUFSIZ];
    int pos;
    int len;
} FILE;

static FILE nk_stdin = {0, {0}, 0, 0};
static FILE nk_stdout = {1, {0}, 0, 0};
static FILE nk_stderr = {2, {0}, 0, 0};
static FILE nk_file = {-1, {0}, 0, 0};
static char stdbuf[BUFSIZ];

static FILE *stdin = &nk_stdin;
static FILE *stdout = &nk_stdout;
static FILE *stderr = &nk_stderr;

static long syscall0(long n)
{
    long out;
    __asm__ volatile("int $0x80" : "=a"(out) : "a"(n) : "rcx", "r11", "memory");
    return out;
}

static long syscall1(long n, long a)
{
    long out;
    __asm__ volatile("int $0x80" : "=a"(out) : "a"(n), "D"(a) : "rcx", "r11", "memory");
    return out;
}

static long syscall3(long n, long a, long b, long c)
{
    long out;
    __asm__ volatile(
        "int $0x80"
        : "=a"(out)
        : "a"(n), "D"(a), "S"(b), "d"(c)
        : "rcx", "r11", "memory");
    return out;
}

static int open(const char *path, int flags)
{
    return (int)syscall3(2, (long)path, flags, 0);
}

static long read(int fd, void *buffer, size_t len)
{
    return syscall3(0, fd, (long)buffer, len);
}

static long write(int fd, const void *buffer, size_t len)
{
    return syscall3(1, fd, (long)buffer, len);
}

static int close(int fd)
{
    return (int)syscall1(3, fd);
}

static void exit(int code)
{
    syscall1(60, code);
    for (;;) {
        syscall0(60);
    }
}

static void setbuf(FILE *stream, char *buffer)
{
    (void)stream;
    (void)buffer;
}

static int fileno(FILE *stream)
{
    return stream->fd;
}

static int fstat(int fd, struct stat *statb)
{
    statb->st_dev = fd;
    statb->st_ino = fd;
    statb->st_mode = S_IFCHR;
    return 0;
}

static FILE *fopen(char *path, char *mode)
{
    int fd;
    (void)mode;
    fd = open(path, 0);
    if (fd < 0) {
        return NULL;
    }
    nk_file.fd = fd;
    nk_file.pos = 0;
    nk_file.len = 0;
    return &nk_file;
}

static int fclose(FILE *stream)
{
    if (stream != stdin && stream != stdout && stream != stderr) {
        close(stream->fd);
    }
    return 0;
}

static int getc(FILE *stream)
{
    long n;
    if (stream->pos >= stream->len) {
        n = read(stream->fd, stream->buf, BUFSIZ);
        if (n <= 0) {
            return EOF;
        }
        stream->pos = 0;
        stream->len = (int)n;
    }
    return stream->buf[stream->pos++];
}

static int putchar(int c)
{
    unsigned char ch = (unsigned char)c;
    write(1, &ch, 1);
    return c;
}

static void writes(FILE *stream, const char *s)
{
    const char *p = s;
    while (*p) {
        p++;
    }
    write(stream->fd, s, (size_t)(p - s));
}

static void fprintf(FILE *stream, const char *fmt, char *arg)
{
    while (*fmt) {
        if (fmt[0] == '%' && fmt[1] == 's') {
            writes(stream, arg);
            fmt += 2;
        } else {
            write(stream->fd, fmt, 1);
            fmt++;
        }
    }
}

static int cat_main(argc, argv)
int argc;
char **argv;
{
    int fflg = 0;
    register FILE *fi;
    register c;
    int dev, ino = -1;
    struct stat statb;
    setbuf(stdout, stdbuf);
    for( ; argc>1 && argv[1][0]=='-'; argc--,argv++) {
        switch(argv[1][1]) {
        case 0:
            break;
        case 'u':
            setbuf(stdout, (char *)NULL);
            continue;
        }
        break;
    }
    fstat(fileno(stdout), &statb);
    statb.st_mode &= S_IFMT;
    if (statb.st_mode!=S_IFCHR && statb.st_mode!=S_IFBLK) {
        dev = statb.st_dev;
        ino = statb.st_ino;
    }
    if (argc < 2) {
        argc = 2;
        fflg++;
    }
    while (--argc > 0) {
        if (fflg || (*++argv)[0]=='-' && (*argv)[1]=='\0')
            fi = stdin;
        else {
            if ((fi = fopen(*argv, "r")) == NULL) {
                fprintf(stderr, "cat: can't open %s\n", *argv);
                continue;
            }
        }
        fstat(fileno(fi), &statb);
        if (statb.st_dev==dev && statb.st_ino==ino) {
            fprintf(stderr, "cat: input %s is output\n",
               fflg?"-": *argv);
            fclose(fi);
            continue;
        }
        while ((c = getc(fi)) != EOF)
            putchar(c);
        if (fi!=stdin)
            fclose(fi);
    }
    return(0);
}

__asm__(
    ".global _start\n"
    "_start:\n"
    "    mov %rsp, %rdi\n"
    "    call nk_start\n");

void nk_start(unsigned long *stack)
{
    int argc = (int)stack[0];
    char **argv = (char **)&stack[1];
    int code = cat_main(argc, argv);
    exit(code);
}

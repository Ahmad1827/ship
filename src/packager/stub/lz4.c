#include <stddef.h>
#include <string.h>

int ship_lz4_decompress(const unsigned char *src, size_t src_len, unsigned char *dest, size_t dest_len) {
    size_t src_pos = 0;
    size_t dest_pos = 0;

    while (src_pos < src_len && dest_pos < dest_len) {
        unsigned char token = src[src_pos++];
        size_t lit_len = token >> 4;

        if (lit_len == 15) {
            unsigned char s = 0;
            do {
                if (src_pos >= src_len) return -1;
                s = src[src_pos++];
                lit_len += s;
            } while (s == 255);
        }

        if (src_pos + lit_len > src_len || dest_pos + lit_len > dest_len) return -2;
        memcpy(dest + dest_pos, src + src_pos, lit_len);
        src_pos += lit_len;
        dest_pos += lit_len;

        if (dest_pos >= dest_len) break;
        if (src_pos + 2 > src_len) return -3;

        unsigned short offset = (unsigned short)(src[src_pos] | (src[src_pos + 1] << 8));
        src_pos += 2;
        if (offset == 0 || (size_t)offset > dest_pos) return -4;

        size_t match_len = (token & 0x0F) + 4;
        if (match_len == 19) {
            unsigned char s = 0;
            do {
                if (src_pos >= src_len) return -5;
                s = src[src_pos++];
                match_len += s;
            } while (s == 255);
        }

        if (dest_pos + match_len > dest_len) return -6;
        for (size_t i = 0; i < match_len; i++) {
            dest[dest_pos] = dest[dest_pos - offset];
            dest_pos++;
        }
    }

    return (dest_pos == dest_len) ? 0 : -7;
}

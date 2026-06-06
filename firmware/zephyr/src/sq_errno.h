#ifndef SQ_ERRNO_H
#define SQ_ERRNO_H

#include <errno.h>

/*
 * Decode a runtime/host status code to its errno name.
 *
 * Codes that flow through the VM result, host callbacks, and the device
 * error/trace reporting are bare integers (e.g. -5, -12). Reporting them raw
 * forces a reader to remember the errno table; pairing the number with a name
 * keeps the diagnostics legible. The caller still prints the number, so the
 * name is supplementary -- an unmapped code is marked "?" rather than hidden.
 *
 * Accepts either sign so callers can pass the value exactly as recorded.
 */
static inline const char *sq_errno_name(int code)
{
	int err = code < 0 ? -code : code;

	switch (err) {
	case 0:
		return "OK";
	case ENOENT:
		return "ENOENT";
	case EIO:
		return "EIO";
	case EAGAIN:
		return "EAGAIN";
	case ENOMEM:
		return "ENOMEM";
	case EACCES:
		return "EACCES";
	case EBUSY:
		return "EBUSY";
	case EEXIST:
		return "EEXIST";
	case ENODEV:
		return "ENODEV";
	case EINVAL:
		return "EINVAL";
	case ENFILE:
		return "ENFILE";
	case EMFILE:
		return "EMFILE";
	case EFBIG:
		return "EFBIG";
	case ENOSPC:
		return "ENOSPC";
	case ENOSYS:
		return "ENOSYS";
	case EMSGSIZE:
		return "EMSGSIZE";
	case ENOTSUP:
		return "ENOTSUP";
	case ETIMEDOUT:
		return "ETIMEDOUT";
	default:
		return "?";
	}
}

#endif /* SQ_ERRNO_H */

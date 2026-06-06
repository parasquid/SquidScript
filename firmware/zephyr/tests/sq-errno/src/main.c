#include <zephyr/ztest.h>

#include "sq_errno.h"

ZTEST_SUITE(sq_errno, NULL, NULL, NULL, NULL, NULL);

/* sq_errno_name turns the opaque numeric codes that flow through the runtime and
 * the device error/trace reporting into readable names, so a reader does not have
 * to remember that -5 is EIO or -12 is ENOMEM. */
ZTEST(sq_errno, test_known_codes_decode_to_names)
{
	zassert_str_equal(sq_errno_name(0), "OK");
	zassert_str_equal(sq_errno_name(-EIO), "EIO");
	zassert_str_equal(sq_errno_name(-ENOMEM), "ENOMEM");
	zassert_str_equal(sq_errno_name(-EINVAL), "EINVAL");
	zassert_str_equal(sq_errno_name(-ENODEV), "ENODEV");
	zassert_str_equal(sq_errno_name(-EBUSY), "EBUSY");
	zassert_str_equal(sq_errno_name(-ENOENT), "ENOENT");
	zassert_str_equal(sq_errno_name(-EFBIG), "EFBIG");
}

/* Codes are normally stored negative; the decode accepts either sign so callers
 * can pass the value as recorded. */
ZTEST(sq_errno, test_sign_is_normalized)
{
	zassert_str_equal(sq_errno_name(EIO), "EIO");
	zassert_str_equal(sq_errno_name(-EIO), "EIO");
}

/* An unmapped code stays representable: the caller still prints the number, so the
 * name is just a marker that it is not one of the known errnos. */
ZTEST(sq_errno, test_unknown_code_is_marked)
{
	zassert_str_equal(sq_errno_name(-31337), "?");
}

package io.github.lucaswilliameufrasio.satchel

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class CreateFlowTest {
    private val id = "0123456789abcdef0123456789abcdef"

    @Test
    fun folderSpecCarriesTheDirectoryAndNoStorageAccount() {
        val spec = createSpecJson(CreateForm(), "/data/files/repository", id)
        assertEquals(
            "{\"kind\":\"folder\",\"device_id\":\"$id\",\"directory\":\"/data/files/repository\"}",
            spec,
        )
        assertFalse(spec.contains("secret"))
    }

    @Test
    fun objectSpecTrimsTheFieldsAndEscapesQuotes() {
        val form = CreateForm(
            storage = StorageChoice.R2,
            endpoint = " https://acc.r2.cloudflarestorage.com ",
            bucket = "b",
            region = "auto",
            prefix = "",
            accessKeyId = "maint",
            secretAccessKey = "a\"b\\c",
            deviceAccessKeyId = "dev",
            deviceSecretAccessKey = "d",
        )
        val spec = createSpecJson(form, "/unused", id)
        assertTrue(spec.contains("\"kind\":\"r2\""))
        assertTrue(spec.contains("\"endpoint\":\"https://acc.r2.cloudflarestorage.com\""))
        assertTrue(spec.contains("\"secret_access_key\":\"a\\\"b\\\\c\""))
        assertFalse(spec.contains("\"directory\""))
    }

    @Test
    fun objectStorageNeedsEveryFieldAndASeparateDeviceKey() {
        val full = CreateForm(
            storage = StorageChoice.B2,
            endpoint = "https://s3.us-west-004.backblazeb2.com",
            bucket = "b",
            region = "us-west-004",
            accessKeyId = "maint",
            secretAccessKey = "s",
            deviceAccessKeyId = "dev",
            deviceSecretAccessKey = "t",
        )
        assertTrue(createFormComplete(full))
        assertFalse(createFormComplete(full.copy(bucket = " ")))
        assertFalse(createFormComplete(full.copy(deviceAccessKeyId = "maint")))
        assertTrue(createFormComplete(CreateForm()))
    }

    @Test
    fun approvalIsEmptyUntilTheCodeIsChecked() {
        assertNull(approvalFrom(""))
        val shown = approvalFrom(
            "{\"deviceName\":\"Mesa\",\"os\":\"macos\",\"peerAddress\":\"100.64.0.3:5\",\"code\":\"123 456\"}",
        )
        assertEquals("Mesa", shown?.deviceName)
        assertEquals("100.64.0.3:5", shown?.peerAddress)
    }

    @Test
    fun controlCharactersAreEscaped() {
        assertEquals("\"a\\nb\"", jsonString("a\nb"))
    }

    @Test
    fun everyRefusalReasonHasItsOwnAdvice() {
        val pt = XmlWords("values-pt")
        val en = XmlWords("values")
        val reasons = listOf("protection", "denied", "exists", "unreachable", "fields", "")
        val texts = reasons.map { pt.text(createFailureCopy(it)) }
        assertEquals(texts.size, texts.toSet().size)
        assertEquals(reasons.size, reasons.map { en.text(createFailureCopy(it)) }.toSet().size)
        assertTrue(pt.text(createFailureCopy("protection")).contains("versionamento"))
        assertTrue(pt.text(createFailureCopy("exists")).contains("prefixo"))
        assertTrue(en.text(createFailureCopy("protection")).contains("versioning"))
        assertTrue(en.text(createFailureCopy("exists")).contains("prefix"))
    }
}

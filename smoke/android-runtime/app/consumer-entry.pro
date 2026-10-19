# Only the test entry point is rooted here. The candidate AAR must supply its
# own JNA reflection, native method, structure and callback keep rules.
-keep class org.lightningdevkit.qualification.CandidateSmoke {
    public static void run(android.content.Context);
}

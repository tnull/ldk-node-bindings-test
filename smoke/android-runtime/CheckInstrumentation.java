import java.nio.file.Files;
import java.nio.file.Path;
import javax.xml.XMLConstants;
import javax.xml.parsers.DocumentBuilderFactory;
import org.w3c.dom.Element;

// Runs directly with JDK 17; no parser dependency or Gradle plugin is needed.
class CheckInstrumentation {
    private static void require(boolean condition, String message) {
        if (!condition) throw new IllegalStateException("Invalid instrumentation evidence: " + message);
    }

    public static void main(String[] args) throws Exception {
        var factory = DocumentBuilderFactory.newInstance();
        factory.setFeature("http://apache.org/xml/features/disallow-doctype-decl", true);
        factory.setAttribute(XMLConstants.ACCESS_EXTERNAL_DTD, "");
        factory.setAttribute(XMLConstants.ACCESS_EXTERNAL_SCHEMA, "");
        var parser = factory.newDocumentBuilder();
        int executed = 0;
        try (var paths = Files.walk(Path.of(args[0]))) {
            var reports = paths.filter(Files::isRegularFile)
                .filter(path -> path.toString().endsWith(".xml")).sorted().toList();
            require(!reports.isEmpty(), "no JUnit XML reports");
            for (var report : reports) {
                var document = parser.parse(report.toFile());
                var root = document.getDocumentElement().getTagName();
                require(root.equals("testsuite") || root.equals("testsuites"), "unexpected XML root");
                for (var state : new String[] {"failure", "error", "skipped"}) {
                    require(document.getElementsByTagName(state).getLength() == 0, state + " in " + report);
                }
                for (var tag : new String[] {"testsuite", "testsuites"}) {
                    var suites = document.getElementsByTagName(tag);
                    for (int i = 0; i < suites.getLength(); i++) {
                        var suite = (Element) suites.item(i);
                        for (var count : new String[] {"tests", "failures", "errors", "skipped", "disabled"}) {
                            int expected = count.equals("tests") ? suite.getElementsByTagName("testcase").getLength() : 0;
                            require(!suite.hasAttribute(count) || suite.getAttribute(count).equals(Integer.toString(expected)),
                                "inconsistent " + count + " count in " + report);
                        }
                    }
                }
                var cases = document.getElementsByTagName("testcase");
                for (int i = 0; i < cases.getLength(); i++) {
                    var test = (Element) cases.item(i);
                    require(test.getAttribute("classname").equals("org.lightningdevkit.qualification.CandidateTest")
                        && test.getAttribute("name").equals("minifiedCandidateStartsStopsAndCallsBack"),
                        "unexpected test case in " + report);
                    executed++;
                }
            }
        }
        require(executed == 1, "expected one successful candidate smoke test, found " + executed);
        System.out.println("Confirmed CandidateTest.minifiedCandidateStartsStopsAndCallsBack passed exactly once");
    }
}

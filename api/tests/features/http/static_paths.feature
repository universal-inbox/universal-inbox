Feature: Paths served by the web app
  The app serves the web application and its static files. Any other path
  answers Not Found, so scanners probing for secrets learn nothing.

  Rule: Paths that cannot be static files are not found

    @legacy:test_misc::invalid_static_paths::test_invalid_static_path_returns_not_found
    Scenario Outline: Requesting <path> answers Not Found
      When I request "<path>"
      Then the app answers 404

      Examples:
        | path                              |
        | /.env                             |
        | /.git/config                      |
        | /.well-known/security.txt         |
        | /*                                |
        | /assets/*.js                      |
        | /c:                               |
        | /foo:/bar                         |
        | /foo%3C                           |
        | /foo%3E                           |
        # Real scanner probes seen in production traces
        | /admin%2F.env                     |
        | /static/%2e%2e%2f%2e%2e%2f.env    |
        | /*%2eenv%2esave                   |
        | /home/user/%2eaws/credentials     |
        | /https://app.universal-inbox.com/ |

    @legacy:test_misc::invalid_static_paths::test_valid_static_path_is_served
    Scenario Outline: Requesting <path> serves the web application
      Unknown routes of the web application fall back to its index page.

      When I request "<path>"
      Then the app answers 200

      Examples:
        | path               |
        | /assets/app.min.js |
        | /notifications/a:b |
        | /foo*bar           |

    @legacy:test_misc::invalid_static_paths::test_registered_well_known_route_still_served
    Scenario: A registered well-known route is still served
      When I request "/.well-known/oauth-authorization-server"
      Then the app answers 200

Feature: Static files of the web app
  The app serves the web application: hashed assets are cached forever and
  served pre-compressed, everything else is revalidated, and the files only
  accept reading.

  Rule: Hashed assets are served pre-compressed and cached forever

    @legacy:test_misc::static_files::test_hashed_asset_is_served_precompressed_and_immutable
    Scenario Outline: A hashed asset is served as <encoding> to a client accepting <accepted encodings>
      When I request "/app-0123456789abcdef_bg.wasm" accepting the encoding "<accepted encodings>"
      Then the app answers 200
      And the response is encoded as "<encoding>"
      And the response header "cache-control" is "public, max-age=31536000, immutable"
      And the response header "content-type" is "application/wasm"
      And the response body is the file "<file>"

      Examples:
        | accepted encodings | encoding | file                                                  |
        | br, gzip           | br       | tests/common/statics/app-0123456789abcdef_bg.wasm.br     |
        | gzip               | gzip     | tests/common/statics/app-0123456789abcdef_bg.wasm.gz     |
        | identity           | identity | tests/common/statics/app-0123456789abcdef_bg.wasm        |

  Rule: Anything else is revalidated on every use

    @legacy:test_misc::static_files::test_unhashed_content_is_revalidated
    Scenario Outline: <path> is revalidated
      When I request "<path>" accepting the encoding "identity"
      Then the app answers 200
      And the response header "cache-control" is "no-cache"

      Examples:
        | path                                |
        | /                                   |
        | /notifications                      |
        | /notifications/app-0123456789abcdef |
        | /style.css                          |

    @legacy:test_misc::static_files::test_spa_fallback_is_served_on_get_and_head
    Scenario Outline: An unknown route of the web app answers a <method> request with the app page
      When I send a <method> request to "/notifications/some-id"
      Then the app answers 200
      And the response header "content-type" is "text/html; charset=utf-8"

      Examples:
        | method |
        | GET    |
        | HEAD   |

  Rule: Static files only accept reading

    @legacy:test_misc::static_files::test_static_content_rejects_other_methods
    Scenario Outline: A <method> request to <path> is not allowed
      When I send a <method> request to "<path>"
      Then the app answers 405
      And the response header "content-type" is not "text/html; charset=utf-8"

      Examples:
        | path                   | method |
        | /                      | POST   |
        | /                      | PUT    |
        | /                      | DELETE |
        | /                      | PATCH  |
        | /notifications/some-id | POST   |
        | /notifications/some-id | PUT    |
        | /notifications/some-id | DELETE |
        | /notifications/some-id | PATCH  |
        | /style.css             | POST   |
        | /style.css             | PUT    |
        | /style.css             | DELETE |
        | /style.css             | PATCH  |

    @legacy:test_misc::static_files::test_spa_fallback_advertises_allowed_methods
    Scenario: A refused method lists the allowed ones
      When I send a POST request to "/notifications/some-id"
      Then the app answers 405
      And the response header "allow" is "GET, HEAD"

    @legacy:test_misc::static_files::test_invalid_static_path_rejects_post_with_not_found
    Scenario: A refused method on a path that cannot be a static file is not found
      When I send a POST request to "/.env"
      Then the app answers 404

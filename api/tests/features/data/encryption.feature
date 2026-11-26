Feature: Encrypting the stored data
  Sensitive values are stored encrypted: OAuth tokens, third-party items, task
  bodies and email addresses. The maintenance commands move all data to a new
  key and encrypt what was stored in plaintext, and an email address stays
  searchable through a blind index.

  Scenario: Rotating the key re-encrypts every value, and the old key alone is refused afterwards
    Given I am signed in
    And I am connected to Todoist
    And my Todoist access token was stored before the encryption envelope existed
    Then the data encryption status counts 1 access token on the legacy key and no value it cannot read
    When the keys are rotated to a second key and all data is re-encrypted
    Then every value that was not on the new key was reported as re-encrypted, and every column as done
    And all data is on the new key, including my access token
    And my access token can be read with the new key
    And starting with the old key alone is refused, naming the column of the access tokens
    And re-encrypting again changes nothing

  Scenario: The data of a third-party item is stored encrypted
    Given I am signed in
    And I am connected to Todoist
    And Todoist can list my projects
    When a Todoist task whose content contains the secret marker is stored
    Then the stored item data does not contain the secret marker
    And the data encryption status counts 1 third-party item value on the first key
    And reading the item back gives the data that was stored

  Scenario: The body of a task is stored encrypted and kept out of the search vector
    Given I am signed in
    And I am connected to Todoist
    And Todoist can list my projects
    When a Todoist task whose description contains the secret marker is stored
    Then the task body is the description
    And the stored task body is encrypted and absent from the search vector
    And reading the task back gives the same body

  Scenario: The raw token response of an OAuth credential is stored encrypted
    Given I am signed in
    And I am connected to Todoist
    When an OAuth credential whose raw token response holds the secret marker is stored
    Then the stored credential reports the raw token response it was given
    And the raw token response is stored encrypted
    And reading the credential back gives the same raw token response

  Scenario: An email address is stored encrypted and found whatever its case
    Given the app lets users sign in with a password or a passkey
    And I have signed up with the email "Jane.Secret@Example.com" and a password
    Then my email address is stored encrypted, next to a blind index of its lowercase form
    When a visitor signs in with the email "jane.secret@EXAMPLE.com" and the password "Very-harD-pasSword-5"
    Then the app answers 200
    And a user with the email "JANE.SECRET@example.com" cannot be created, as it already exists
    And looking up the address "jane.secret@example.com" finds me, with my email address as it was written

  Scenario: Encrypting the plaintext values turns a Google account email into its blind index
    Given I am signed in
    And I have a Google Mail connection pinned to the address "john.roe@example.com" before addresses were encrypted
    Then the data encryption status counts 1 plaintext Google email value, as does the list of plaintext counts
    When all plaintext values are encrypted
    Then the progress totals are the plaintext counts and every counted column is done
    And no plaintext value is left
    And the Google Mail connection is pinned to the blind index of "john.roe@example.com"
